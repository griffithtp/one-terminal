#!/usr/bin/env tsx
/**
 * Interactive migration wizard.
 *
 * 1. Re-extracts templates to a temp dir
 * 2. Diffs against committed templates to detect changes
 * 3. For each changed file, infers migration type and prompts for spec details
 * 4. Bumps the version in versions.json and prepends a new entry
 *    (or amends the version already added on this branch)
 * 5. Updates scaffoldVersion in context.ts so new workspaces start on the right version
 *    (skipped when amending — version hasn't changed)
 * 6. Copies the new templates to the committed templates directory
 *
 * Usage:
 *   npm run create-migration
 */

import * as p from "@clack/prompts";
import pc from "picocolors";
import { execSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { cp, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const TEMPLATES = join(ROOT, "packages/create-one-terminal/templates");
const VERSIONS_JSON = join(ROOT, "packages/create-one-terminal/versions.json");
const CONTEXT_TS = join(ROOT, "packages/create-one-terminal/src/create/context.ts");

// ── Inline types (mirrors migrations/types.ts to avoid import chain) ─────────

type PatchOperation =
  | { op: "insert-after-line-matching"; pattern: string; content: string }
  | { op: "replace-line-matching"; pattern: string; replacement: string }
  | { op: "add-file"; sourcePath: string; targetPath: string };

type MigrationSpec =
  | {
      type: "config-merge";
      id: string;
      target: string;
      description: string;
      patch: Record<string, unknown>;
    }
  | {
      type: "dep-bump";
      id: string;
      target: string;
      description: string;
      deps: Array<{ name: string; ecosystem: "cargo" | "npm"; newVersion: string }>;
    }
  | {
      type: "structural";
      id: string;
      target: string;
      description: string;
      operations: PatchOperation[];
    };

interface VersionEntry {
  version: string;
  releaseDate: string;
  breaking: boolean;
  changelogUrl: string;
  migrations: MigrationSpec[];
}

interface VersionsManifest {
  schemaVersion: 1;
  versions: VersionEntry[];
}

type MigrationKind = "config-merge" | "dep-bump" | "structural";

interface FileChange {
  relPath: string;
  oldContent: string | null;
  newContent: string;
  kind: MigrationKind;
}

interface DepBump {
  name: string;
  oldVersion: string;
  newVersion: string;
  ecosystem: "cargo" | "npm";
}

interface JsonDiff {
  path: string;
  type: "added" | "changed" | "removed";
  old: unknown;
  new: unknown;
}

// ── Main ─────────────────────────────────────────────────────────────────────

p.intro(pc.bgCyan(pc.black(" OneTerminal — Migration Wizard ")));

// Step 1: Extract templates to temp dir
const spin = p.spinner();
spin.start("Extracting templates from source apps…");
const tmpDir = mkdtempSync(join(tmpdir(), "ot-migrate-"));

try {
  execSync(`npx tsx ${join(ROOT, "scripts/extract-templates.ts")} --output ${tmpDir}`, {
    cwd: ROOT,
    stdio: "pipe",
  });
} catch (err) {
  spin.stop("Extraction failed.");
  cleanup();
  p.cancel(String(err));
  process.exit(1);
}
spin.stop("Templates extracted.");

// Step 2: Diff against committed templates
const changes = await detectChanges(tmpDir);

if (changes.length === 0) {
  cleanup();
  p.outro("No template changes detected — nothing to migrate.");
  process.exit(0);
}

p.log.info(`${pc.bold(String(changes.length))} file(s) changed.`);

// Step 3: Read current manifest
const manifest = JSON.parse(await readFile(VERSIONS_JSON, "utf8")) as VersionsManifest;
const latestEntry = [...manifest.versions].sort((a, b) =>
  semverGt(a.version, b.version) ? -1 : 1
)[0];
const currentVersion = latestEntry?.version ?? "0.1.0";

// Step 4: Detect whether this branch already added a migration version and offer to amend it
const branchVersion = detectBranchMigration(manifest);

type Mode = "amend" | "new";
let mode: Mode = "new";
let amendEntry: VersionEntry | null = null;

if (branchVersion) {
  const choice = await p.select({
    message: `Version ${pc.bold(branchVersion)} was already added on this branch. What would you like to do?`,
    options: [
      {
        value: "amend",
        label: `Amend v${branchVersion} — merge new migrations into this version`,
      },
      { value: "new", label: "Create a new version on top" },
    ],
  });
  if (p.isCancel(choice)) cancel();
  mode = choice as Mode;
  if (mode === "amend") {
    amendEntry = manifest.versions.find((v) => v.version === branchVersion) ?? null;
    if (!amendEntry) {
      p.log.warn("Could not locate branch version entry — falling back to new version.");
      mode = "new";
    }
  }
}

// Step 5: Determine target version
let newVersion: string;
let isBreaking: boolean;

if (mode === "amend" && amendEntry) {
  newVersion = amendEntry.version;
  isBreaking = amendEntry.breaking;
  p.log.info(`Amending v${newVersion}…`);
} else {
  const bumpType = await p.select({
    message: `Current version is ${pc.bold(currentVersion)}. Choose bump type:`,
    options: [
      {
        value: "patch",
        label: `patch  →  ${bumpVersion(currentVersion, "patch")}  (bug fixes, non-breaking additions)`,
      },
      {
        value: "minor",
        label: `minor  →  ${bumpVersion(currentVersion, "minor")}  (new functionality, backwards-compatible)`,
      },
      {
        value: "major",
        label: `major  →  ${bumpVersion(currentVersion, "major")}  (breaking changes)`,
      },
    ],
  });
  if (p.isCancel(bumpType)) cancel();
  newVersion = bumpVersion(currentVersion, bumpType as "patch" | "minor" | "major");

  const breakingAnswer = await p.confirm({
    message: "Does this version contain breaking changes for existing scaffolded workspaces?",
    initialValue: bumpType === "major",
  });
  if (p.isCancel(breakingAnswer)) cancel();
  isBreaking = breakingAnswer as boolean;
}

// Step 6: Per-file migration prompts
const migrations: MigrationSpec[] = [];

for (let i = 0; i < changes.length; i++) {
  const change = changes[i];
  const kindLabel = {
    "config-merge": "JSON config",
    "dep-bump": "dependency versions",
    structural: "source code",
  }[change.kind];

  p.log.step(
    `${pc.bold(`${i + 1} / ${changes.length}`)}  ${pc.dim(change.relPath)}  ${pc.cyan(`[${kindLabel}]`)}`
  );

  let spec: MigrationSpec | null = null;
  if (change.kind === "config-merge") spec = await promptConfigMerge(change, newVersion);
  else if (change.kind === "dep-bump") spec = await promptDepBump(change, newVersion);
  else spec = await promptStructural(change, newVersion);

  if (spec) {
    migrations.push(spec);
    p.log.success(`Migration added: ${pc.bold(spec.id)}`);
  } else {
    p.log.info("Skipped.");
  }
}

// Step 7: Write versions.json
if (mode === "amend" && amendEntry) {
  // Merge new specs into existing entry (update by id, append if new)
  for (const spec of migrations) {
    const idx = amendEntry.migrations.findIndex((m) => m.id === spec.id);
    if (idx >= 0) {
      amendEntry.migrations[idx] = spec;
    } else {
      amendEntry.migrations.push(spec);
    }
  }
  const entryIdx = manifest.versions.findIndex((v) => v.version === amendEntry!.version);
  if (entryIdx >= 0) manifest.versions[entryIdx] = amendEntry;
  await writeFile(VERSIONS_JSON, JSON.stringify(manifest, null, 2) + "\n", "utf8");
  p.log.success(
    `versions.json  →  v${newVersion} amended  (${amendEntry.migrations.length} total migration(s))`
  );
} else {
  const newEntry: VersionEntry = {
    version: newVersion,
    releaseDate: new Date().toISOString().slice(0, 10),
    breaking: isBreaking,
    changelogUrl: `https://github.com/one-terminal/one-terminal/releases/tag/v${newVersion}`,
    migrations,
  };
  manifest.versions.unshift(newEntry);
  await writeFile(VERSIONS_JSON, JSON.stringify(manifest, null, 2) + "\n", "utf8");
  p.log.success(`versions.json  →  v${newVersion}  (${migrations.length} migration(s))`);
}

// Step 8: Update scaffoldVersion in context.ts (new versions only — amend keeps the same version)
if (mode === "new") {
  const ctxSource = await readFile(CONTEXT_TS, "utf8");
  const updatedCtx = ctxSource.replace(
    /scaffoldVersion:\s*["'][^"']+["']/,
    `scaffoldVersion: "${newVersion}"`
  );
  if (updatedCtx !== ctxSource) {
    await writeFile(CONTEXT_TS, updatedCtx, "utf8");
    p.log.success(`context.ts  →  scaffoldVersion: "${newVersion}"`);
  }
}

// Step 9: Copy new templates to committed location
await cp(tmpDir, TEMPLATES, { recursive: true, force: true });
cleanup();
p.log.success("Committed templates updated.");

if (mode === "amend") {
  p.outro(
    [
      `v${newVersion} amended (+${migrations.length} migration(s)).`,
      "",
      `Run ${pc.bold("npm run build:scaffolder")} then test with:`,
      `  node packages/create-one-terminal/dist/index.js upgrade`,
    ].join("\n")
  );
} else {
  p.outro(
    [
      `v${currentVersion} → v${newVersion} complete.`,
      "",
      `Run ${pc.bold("npm run build:scaffolder")} then test with:`,
      `  node packages/create-one-terminal/dist/index.js upgrade`,
    ].join("\n")
  );
}

// ── Branch migration detection ────────────────────────────────────────────────

function detectBranchMigration(currentManifest: VersionsManifest): string | null {
  const currentLatest = [...currentManifest.versions].sort((a, b) =>
    semverGt(a.version, b.version) ? -1 : 1
  )[0]?.version;
  if (!currentLatest) return null;

  let mainContent: string;
  try {
    mainContent = execSync(`git show main:packages/create-one-terminal/versions.json`, {
      cwd: ROOT,
      encoding: "utf8",
      stdio: "pipe",
    });
  } catch {
    try {
      mainContent = execSync(`git show origin/main:packages/create-one-terminal/versions.json`, {
        cwd: ROOT,
        encoding: "utf8",
        stdio: "pipe",
      });
    } catch {
      return null;
    }
  }

  const mainManifest = JSON.parse(mainContent) as VersionsManifest;
  const mainLatest = [...mainManifest.versions].sort((a, b) =>
    semverGt(a.version, b.version) ? -1 : 1
  )[0]?.version;

  return currentLatest !== mainLatest ? currentLatest : null;
}

// ── Change detection ──────────────────────────────────────────────────────────

async function detectChanges(newDir: string): Promise<FileChange[]> {
  const out: FileChange[] = [];
  await walkAndCompare(newDir, TEMPLATES, "", out);
  return out;
}

async function walkAndCompare(
  newDir: string,
  oldDir: string,
  rel: string,
  out: FileChange[]
): Promise<void> {
  const items = await readdir(newDir).catch(() => [] as string[]);
  for (const item of items) {
    const relPath = rel ? `${rel}/${item}` : item;
    const newPath = join(newDir, item);
    const oldPath = join(oldDir, item);
    const info = await stat(newPath);
    if (info.isDirectory()) {
      await walkAndCompare(newPath, oldPath, relPath, out);
    } else {
      const newContent = await readFile(newPath, "utf8").catch(() => null);
      const oldContent = await readFile(oldPath, "utf8").catch(() => null);
      if (!newContent) continue;
      if (newContent !== oldContent) {
        out.push({ relPath, oldContent, newContent, kind: classifyChange(relPath) });
      }
    }
  }
}

function classifyChange(relPath: string): MigrationKind {
  const base = basename(relPath.replace(/\.ejs$/, ""));
  if (base === "Cargo.toml" || base === "package.json") return "dep-bump";
  if (extname(base) === ".json") return "config-merge";
  return "structural";
}

// ── Migration prompts ─────────────────────────────────────────────────────────

async function promptConfigMerge(
  change: FileChange,
  version: string
): Promise<MigrationSpec | null> {
  const oldJson = change.oldContent ? parseEjsJson(change.oldContent) : {};
  const newJson = parseEjsJson(change.newContent);
  const diffs = jsonDiff(oldJson, newJson);

  if (diffs.length > 0) {
    const lines = diffs.map((d) => {
      if (d.type === "added")
        return `  ${pc.green("+")} ${d.path}: ${pc.bold(JSON.stringify(d.new))}`;
      if (d.type === "removed") return `  ${pc.red("-")} ${d.path}`;
      return `  ${pc.yellow("~")} ${d.path}: ${JSON.stringify(d.old)} → ${pc.bold(JSON.stringify(d.new))}`;
    });
    p.log.message(lines.join("\n"));
  } else {
    p.log.warn("Could not auto-detect JSON-level changes (EJS expressions may prevent parsing).");
    p.log.message(getLineDiff(change.oldContent ?? "", change.newContent));
  }

  const include = await p.confirm({
    message: "Include a config-merge migration for this file?",
    initialValue: diffs.length > 0,
  });
  if (p.isCancel(include) || !include) return null;

  const target = change.relPath.replace(/\.ejs$/, "");

  const id = await p.text({
    message: "Migration ID",
    initialValue: `${version}-${slugify(basename(target))}`,
  });
  if (p.isCancel(id)) return null;

  const description = await p.text({
    message: "Description",
    initialValue:
      diffs.length > 0
        ? `Update ${basename(target)}: ${diffs
            .filter((d) => d.type !== "removed")
            .map((d) => d.path)
            .join(", ")}`
        : `Update ${basename(target)}`,
  });
  if (p.isCancel(description)) return null;

  // Build patch from added/changed fields
  const patch: Record<string, unknown> = {};
  for (const d of diffs.filter((x) => x.type !== "removed")) {
    setNestedValue(patch, d.path.split("."), d.new);
  }

  return {
    type: "config-merge",
    id: id as string,
    target,
    description: description as string,
    patch,
  };
}

async function promptDepBump(change: FileChange, version: string): Promise<MigrationSpec | null> {
  const isCargo = change.relPath.replace(/\.ejs$/, "").endsWith("Cargo.toml");
  const bumps = isCargo
    ? detectCargoBumps(change.oldContent ?? "", change.newContent)
    : detectNpmBumps(change.oldContent ?? "", change.newContent);

  if (bumps.length > 0) {
    const lines = bumps.map(
      (b) =>
        `  ${pc.cyan(b.name)}  ${pc.dim(b.oldVersion)} → ${pc.green(b.newVersion)}  ${pc.dim(`(${b.ecosystem})`)}`
    );
    p.log.message(lines.join("\n"));
  } else {
    p.log.warn("No version changes auto-detected.");
    p.log.message(getLineDiff(change.oldContent ?? "", change.newContent));
  }

  const include = await p.confirm({
    message: "Include a dep-bump migration for this file?",
    initialValue: bumps.length > 0,
  });
  if (p.isCancel(include) || !include) return null;

  const target = change.relPath.replace(/\.ejs$/, "");

  const id = await p.text({
    message: "Migration ID",
    initialValue: `${version}-dep-bump-${isCargo ? "cargo" : "npm"}`,
  });
  if (p.isCancel(id)) return null;

  const description = await p.text({
    message: "Description",
    initialValue:
      bumps.length > 0
        ? `Bump ${bumps.map((b) => b.name).join(", ")}`
        : `Bump ${isCargo ? "Cargo" : "npm"} dependencies`,
  });
  if (p.isCancel(description)) return null;

  return {
    type: "dep-bump",
    id: id as string,
    target,
    description: description as string,
    deps: bumps.map((b) => ({ name: b.name, ecosystem: b.ecosystem, newVersion: b.newVersion })),
  };
}

async function promptStructural(
  change: FileChange,
  version: string
): Promise<MigrationSpec | null> {
  const isNewFile = change.oldContent === null;

  if (isNewFile) {
    p.log.message(
      pc.green("(new file)") +
        "\n" +
        pc.dim(
          change.newContent
            .split("\n")
            .slice(0, 8)
            .map((l) => `+ ${l}`)
            .join("\n")
        )
    );
  } else {
    p.log.message(pc.dim(getLineDiff(change.oldContent ?? "", change.newContent)));
  }

  const include = await p.confirm({
    message: "Include a structural migration for this file?",
    initialValue: isNewFile,
  });
  if (p.isCancel(include) || !include) return null;

  const target = change.relPath.replace(/\.ejs$/, "");

  const id = await p.text({
    message: "Migration ID",
    initialValue: `${version}-${slugify(basename(target))}`,
  });
  if (p.isCancel(id)) return null;

  const description = await p.text({ message: "Description" });
  if (p.isCancel(description)) return null;

  const opTypeOptions = [
    { value: "insert-after-line-matching", label: "Insert a line after a matching line" },
    { value: "replace-line-matching", label: "Replace a matching line" },
    ...(isNewFile
      ? [{ value: "add-file", label: "Add new file (copy from bundled template)" }]
      : []),
  ];

  const opType = await p.select({ message: "Operation type", options: opTypeOptions });
  if (p.isCancel(opType)) return null;

  let op: PatchOperation;
  if (opType === "add-file") {
    op = { op: "add-file", sourcePath: target, targetPath: target };
  } else if (opType === "insert-after-line-matching") {
    const pattern = await p.text({
      message: "Pattern — unique substring of the line to anchor on",
      placeholder: "e.g. .plugin(ot_fdc3::init())",
    });
    if (p.isCancel(pattern)) return null;
    const content = await p.text({
      message: "Line content to insert (exact indentation included)",
    });
    if (p.isCancel(content)) return null;
    op = {
      op: "insert-after-line-matching",
      pattern: pattern as string,
      content: content as string,
    };
  } else {
    const pattern = await p.text({
      message: "Pattern — unique substring of the line to anchor on",
      placeholder: "e.g. .plugin(ot_fdc3::init())",
    });
    if (p.isCancel(pattern)) return null;
    const replacement = await p.text({ message: "Replacement line (exact indentation included)" });
    if (p.isCancel(replacement)) return null;
    op = {
      op: "replace-line-matching",
      pattern: pattern as string,
      replacement: replacement as string,
    };
  }

  return {
    type: "structural",
    id: id as string,
    target,
    description: description as string,
    operations: [op],
  };
}

// ── Diff helpers ──────────────────────────────────────────────────────────────

function getLineDiff(oldContent: string, newContent: string): string {
  const tmpOld = join(tmpdir(), `ot-diff-old-${Date.now()}`);
  const tmpNew = join(tmpdir(), `ot-diff-new-${Date.now()}`);
  try {
    writeFileSync(tmpOld, oldContent);
    writeFileSync(tmpNew, newContent);
    // tail -n +3 strips the --- / +++ header lines; head -40 keeps output readable
    return (
      execSync(`diff -u "${tmpOld}" "${tmpNew}" | tail -n +3 | head -40 || true`, {
        encoding: "utf8",
      }).trim() || "(identical after normalisation)"
    );
  } finally {
    rmSync(tmpOld, { force: true });
    rmSync(tmpNew, { force: true });
  }
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

function normalizeEjsForJson(content: string): string {
  // Numeric context: `"port": <%= expr %>` → `"port": 0`
  content = content.replace(/:\s*<%=\s*[\w.]+\s*%>/g, ": 0");
  // String context: `"<%= expr %>"` or `<%= expr %>` inside a string value
  content = content.replace(/<%=\s*[\w.]+\s*%>/g, "PLACEHOLDER");
  // Remove block tags entirely
  content = content.replace(/<%[^>]*%>/g, "");
  return content;
}

function parseEjsJson(content: string): Record<string, unknown> {
  try {
    return JSON.parse(normalizeEjsForJson(content)) as Record<string, unknown>;
  } catch {
    return {};
  }
}

function jsonDiff(
  oldObj: Record<string, unknown>,
  newObj: Record<string, unknown>,
  prefix = ""
): JsonDiff[] {
  const results: JsonDiff[] = [];
  const keys = new Set([...Object.keys(oldObj), ...Object.keys(newObj)]);
  for (const key of keys) {
    if (key === "_managed") continue;
    const path = prefix ? `${prefix}.${key}` : key;
    if (!(key in oldObj)) {
      results.push({ path, type: "added", old: undefined, new: newObj[key] });
    } else if (!(key in newObj)) {
      results.push({ path, type: "removed", old: oldObj[key], new: undefined });
    } else if (
      typeof oldObj[key] === "object" &&
      oldObj[key] !== null &&
      typeof newObj[key] === "object" &&
      newObj[key] !== null &&
      !Array.isArray(oldObj[key])
    ) {
      results.push(
        ...jsonDiff(
          oldObj[key] as Record<string, unknown>,
          newObj[key] as Record<string, unknown>,
          path
        )
      );
    } else if (JSON.stringify(oldObj[key]) !== JSON.stringify(newObj[key])) {
      results.push({ path, type: "changed", old: oldObj[key], new: newObj[key] });
    }
  }
  return results;
}

function setNestedValue(obj: Record<string, unknown>, path: string[], value: unknown): void {
  const [head, ...rest] = path;
  if (rest.length === 0) {
    obj[head] = value;
  } else {
    if (typeof obj[head] !== "object" || obj[head] === null) obj[head] = {};
    setNestedValue(obj[head] as Record<string, unknown>, rest, value);
  }
}

// ── Dep-bump detection ────────────────────────────────────────────────────────

function detectCargoBumps(oldContent: string, newContent: string): DepBump[] {
  const pattern = /^([\w-]+)\s*=\s*\{[^}]*?version\s*=\s*"([^"]+)"/gm;
  const oldV = new Map<string, string>();
  const newV = new Map<string, string>();
  for (const m of oldContent.matchAll(pattern)) oldV.set(m[1], m[2]);
  for (const m of newContent.matchAll(pattern)) newV.set(m[1], m[2]);
  const bumps: DepBump[] = [];
  for (const [name, nv] of newV) {
    const ov = oldV.get(name);
    if (ov && ov !== nv) bumps.push({ name, oldVersion: ov, newVersion: nv, ecosystem: "cargo" });
  }
  return bumps;
}

function detectNpmBumps(oldContent: string, newContent: string): DepBump[] {
  const old = parseEjsJson(oldContent) as Record<string, Record<string, string>>;
  const next = parseEjsJson(newContent) as Record<string, Record<string, string>>;
  const bumps: DepBump[] = [];
  for (const section of ["dependencies", "devDependencies"] as const) {
    const o = old[section] ?? {};
    const n = next[section] ?? {};
    for (const [name, nv] of Object.entries(n)) {
      const ov = o[name];
      if (ov && ov !== nv) {
        const cleanOld = ov.replace(/^[\^~>=<]+/, "");
        const cleanNew = nv.replace(/^[\^~>=<]+/, "");
        if (cleanOld !== cleanNew)
          bumps.push({ name, oldVersion: cleanOld, newVersion: cleanNew, ecosystem: "npm" });
      }
    }
  }
  return bumps;
}

// ── Version helpers ───────────────────────────────────────────────────────────

function bumpVersion(current: string, type: "patch" | "minor" | "major"): string {
  const [maj, min, pat] = current.split(".").map(Number);
  if (type === "major") return `${maj + 1}.0.0`;
  if (type === "minor") return `${maj}.${min + 1}.0`;
  return `${maj}.${min}.${pat + 1}`;
}

function semverGt(a: string, b: string): boolean {
  const [aMaj, aMin, aPat] = a.split(".").map(Number);
  const [bMaj, bMin, bPat] = b.split(".").map(Number);
  if (aMaj !== bMaj) return aMaj > bMaj;
  if (aMin !== bMin) return aMin > bMin;
  return aPat > bPat;
}

// ── Misc ──────────────────────────────────────────────────────────────────────

function slugify(s: string): string {
  return s
    .replace(/[^a-z0-9]+/gi, "-")
    .toLowerCase()
    .replace(/^-|-$/g, "");
}

function cleanup(): void {
  rmSync(tmpDir, { recursive: true, force: true });
}

function cancel(): never {
  p.cancel("Cancelled.");
  cleanup();
  process.exit(0);
}
