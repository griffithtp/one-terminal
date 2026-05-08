import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { parse, stringify } from "smol-toml";
import type { MigrationResult } from "../upgrade/migrations/types.js";

export async function applyDepBump(
  cwd: string,
  target: string,
  deps: Array<{ name: string; ecosystem: "cargo" | "npm"; newVersion: string }>,
  migrationId: string,
  description: string
): Promise<MigrationResult> {
  const filePath = join(cwd, target);
  const raw = await readFile(filePath, "utf8");

  let bumped = false;

  const cargoDeps = deps.filter((d) => d.ecosystem === "cargo");
  const npmDeps = deps.filter((d) => d.ecosystem === "npm");

  let content = raw;

  if (cargoDeps.length > 0) {
    const toml = parse(raw) as Record<string, unknown>;
    const wsDeps = (toml["workspace"] as Record<string, unknown> | undefined)?.["dependencies"] as
      | Record<string, unknown>
      | undefined;

    if (wsDeps) {
      for (const { name, newVersion } of cargoDeps) {
        const entry = wsDeps[name];
        if (entry === undefined) continue;
        if (typeof entry === "string") {
          wsDeps[name] = newVersion;
          bumped = true;
        } else if (typeof entry === "object" && entry !== null) {
          (entry as Record<string, unknown>)["version"] = newVersion;
          bumped = true;
        }
      }
      content = stringify(toml);
    }
  }

  if (npmDeps.length > 0) {
    const pkg = JSON.parse(content) as Record<string, Record<string, string>>;
    for (const { name, newVersion } of npmDeps) {
      for (const section of ["dependencies", "devDependencies"] as const) {
        if (pkg[section]?.[name] !== undefined) {
          pkg[section][name] = `^${newVersion}`;
          bumped = true;
        }
      }
    }
    content = JSON.stringify(pkg, null, 2) + "\n";
  }

  if (bumped) {
    await writeFile(filePath, content, "utf8");
    return { id: migrationId, description, status: "applied" };
  }

  return { id: migrationId, description, status: "skipped", detail: "No matching deps found" };
}
