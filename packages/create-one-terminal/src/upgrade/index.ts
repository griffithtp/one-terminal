import { join, dirname } from "node:path";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import * as p from "@clack/prompts";
import { detectProject } from "./detect.js";
import { fetchManifest } from "./manifest.js";
import { buildChain, latestVersion } from "./chain.js";
import { snapshot } from "./backup.js";
import { restore } from "./restore.js";
import { writeReport } from "./report.js";
import { applyConfigMerge } from "../merge/json-deep-merge.js";
import { applyDepBump } from "../merge/toml-dep-bump.js";
import type { MigrationResult, MigrationSpec } from "./migrations/types.js";

export async function runUpgrade(cwd = process.cwd()): Promise<void> {
  p.intro("OneTerminal Upgrade");

  const project = await detectProject(cwd).catch((err: Error) => {
    p.cancel(err.message);
    process.exit(1);
  });

  const spin = p.spinner();
  spin.start("Fetching versions manifest…");
  const manifest = await fetchManifest().catch(() => null);
  if (!manifest) {
    spin.stop("Could not fetch manifest.");
    p.cancel("Unable to load versions.json. Check your network and try again.");
    process.exit(1);
  }
  spin.stop("Manifest loaded.");

  const chain = buildChain(manifest, project.version, project.variant);
  const targetVersion = latestVersion(manifest);

  if (chain.length === 0) {
    p.outro(`Already up to date (v${project.version}).`);
    return;
  }

  p.note(
    [
      `Upgrading from v${project.version} → v${targetVersion}`,
      "",
      ...chain.map(
        ({ version, migration }) => `  • [v${version}] ${migration.id}  ${migration.description}`
      ),
    ].join("\n"),
    `${chain.length} migration(s)`
  );

  const go = await p.confirm({ message: "Apply migrations?", initialValue: true });
  if (p.isCancel(go) || !go) {
    p.cancel("Upgrade cancelled.");
    return;
  }

  // Snapshot all targets before touching anything
  const allTargets = [...new Set(chain.map(({ migration }) => migration.target))];
  await snapshot(cwd, targetVersion, allTargets);

  const results: MigrationResult[] = [];

  try {
    for (const { version, migration } of chain) {
      const result = await applyMigration(cwd, migration, version);
      results.push(result);
    }
  } catch (err) {
    p.cancel(`Migration failed: ${String(err)}\nRolling back…`);
    await restore(cwd, targetVersion);
    p.note("Rollback complete. Project restored to original state.", "Rolled back");
    process.exit(1);
  }

  // Update oneTerminal.version in package.json
  const pkgPath = join(cwd, "package.json");
  const pkg = JSON.parse(await readFile(pkgPath, "utf8")) as Record<string, unknown>;
  (pkg["oneTerminal"] as Record<string, string>)["version"] = targetVersion;
  await writeFile(pkgPath, JSON.stringify(pkg, null, 2) + "\n", "utf8");

  await writeReport(cwd, project.version, targetVersion, results);

  const applied = results.filter((r) => r.status === "applied").length;
  const manual = results.filter((r) => r.status === "needs-manual").length;

  p.outro(
    [
      `Upgraded to v${targetVersion}`,
      `${applied} applied, ${results.length - applied} skipped/manual`,
      manual > 0 ? "See upgrade-report.md for items needing manual review." : "",
    ]
      .filter(Boolean)
      .join("\n")
  );
}

async function applyMigration(
  cwd: string,
  migration: MigrationSpec,
  version: string
): Promise<MigrationResult> {
  if (migration.type === "config-merge") {
    return applyConfigMerge(
      cwd,
      migration.target,
      migration.patch,
      migration.id,
      migration.description,
      version
    );
  }

  if (migration.type === "dep-bump") {
    return applyDepBump(cwd, migration.target, migration.deps, migration.id, migration.description);
  }

  if (migration.type === "structural") {
    return applyStructural(cwd, migration, version);
  }

  return { id: (migration as MigrationSpec).id, description: "", status: "skipped" };
}

async function applyStructural(
  cwd: string,
  migration: Extract<MigrationSpec, { type: "structural" }>,
  _version: string
): Promise<MigrationResult> {
  const templatesDir = join(fileURLToPath(import.meta.url), "../..", "templates");
  // Templates live under per-variant overlays; try each in order.
  // shared first (most common), then enterprise/standalone for variant-specific files.
  const OVERLAYS = ["shared", "enterprise", "standalone"];

  async function readOverlaySource(sourcePath: string): Promise<string | null> {
    for (const overlay of OVERLAYS) {
      for (const candidate of [sourcePath + ".ejs", sourcePath]) {
        try {
          return await readFile(join(templatesDir, overlay, candidate), "utf8");
        } catch {
          // try next
        }
      }
    }
    return null;
  }

  // add-file ops create new files — handle them before any readFile on migration.target
  for (const op of migration.operations) {
    if (op.op !== "add-file") continue;

    const srcContent = await readOverlaySource(op.sourcePath);
    if (srcContent === null) {
      return {
        id: migration.id,
        description: migration.description,
        status: "needs-manual",
        detail: `Template source not found: ${op.sourcePath}`,
      };
    }

    const destPath = join(cwd, op.targetPath);
    await mkdir(dirname(destPath), { recursive: true });
    await writeFile(destPath, srcContent, "utf8");
  }

  const patchOps = migration.operations.filter((op) => op.op !== "add-file");
  if (patchOps.length === 0) {
    return { id: migration.id, description: migration.description, status: "applied" };
  }

  // Patch ops modify an existing file — read it once, then apply all ops in order
  const filePath = join(cwd, migration.target);
  let content = await readFile(filePath, "utf8");
  let changed = false;

  for (const op of patchOps) {
    if (op.op === "insert-after-line-matching") {
      const lines = content.split("\n");
      const idx = lines.findIndex((l) => l.includes(op.pattern));
      if (idx === -1) {
        return {
          id: migration.id,
          description: migration.description,
          status: "needs-manual",
          detail: `Pattern not found: ${op.pattern}`,
        };
      }
      lines.splice(idx + 1, 0, op.content);
      content = lines.join("\n");
      changed = true;
    } else if (op.op === "replace-line-matching") {
      const lines = content.split("\n");
      const idx = lines.findIndex((l) => l.includes(op.pattern));
      if (idx === -1) {
        return {
          id: migration.id,
          description: migration.description,
          status: "needs-manual",
          detail: `Pattern not found: ${op.pattern}`,
        };
      }
      lines[idx] = op.replacement;
      content = lines.join("\n");
      changed = true;
    }
  }

  if (changed) {
    await writeFile(filePath, content, "utf8");
  }

  return { id: migration.id, description: migration.description, status: "applied" };
}
