import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { handleConflict } from "../upgrade/conflict.js";
import { addManagedPath, getManagedPaths, getNestedValue, setNestedValue } from "./managed.js";
import type { MigrationResult } from "../upgrade/migrations/types.js";

export async function applyConfigMerge(
  cwd: string,
  target: string,
  patch: Record<string, unknown>,
  migrationId: string,
  description: string,
  frameworkVersion: string
): Promise<MigrationResult> {
  const filePath = join(cwd, target);
  const raw = await readFile(filePath, "utf8");
  const config = JSON.parse(raw) as Record<string, unknown>;
  const managed = getManagedPaths(config);

  for (const [dotPath, newValue] of flattenObject(patch)) {
    const currentValue = getNestedValue(config, dotPath);

    if (currentValue === undefined) {
      // New field — just add it
      setNestedValue(config, dotPath, newValue);
      addManagedPath(config, dotPath);
    } else if (managed.includes(dotPath)) {
      // Framework-owned field — check if user modified it
      if (JSON.stringify(currentValue) !== JSON.stringify(newValue)) {
        const choice = await handleConflict({
          label: migrationId,
          filePath: target,
          fieldPath: dotPath,
          yourValue: JSON.stringify(currentValue),
          frameworkValue: JSON.stringify(newValue),
          frameworkVersion,
        });

        if (choice === "auto") {
          setNestedValue(config, dotPath, newValue);
        } else if (choice === "merge") {
          return {
            id: migrationId,
            description,
            status: "needs-manual",
            detail: `See ${target}.merge`,
          };
        } else {
          return { id: migrationId, description, status: "skipped" };
        }
      }
    }
    // If not in managed and not undefined, it's user-owned — never touch it
  }

  await writeFile(filePath, JSON.stringify(config, null, 2) + "\n", "utf8");
  return { id: migrationId, description, status: "applied" };
}

function flattenObject(obj: Record<string, unknown>, prefix = ""): Array<[string, unknown]> {
  const result: Array<[string, unknown]> = [];
  for (const [key, value] of Object.entries(obj)) {
    const dotPath = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      result.push(...flattenObject(value as Record<string, unknown>, dotPath));
    } else {
      result.push([dotPath, value]);
    }
  }
  return result;
}
