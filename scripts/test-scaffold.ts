#!/usr/bin/env tsx
/**
 * Smoke-tests the scaffolder by rendering a workspace to a temp directory
 * and running `cargo check --workspace` against it.
 *
 * Usage:
 *   npx tsx scripts/test-scaffold.ts
 *   npx tsx scripts/test-scaffold.ts --keep   # don't delete the output on success
 */

import { execSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const keep = process.argv.includes("--keep");

// Import the compiled scaffolder directly — bypasses the interactive prompts.
const { renderWorkspace } = await import(
  join(ROOT, "packages/create-one-terminal/dist/create/render.js")
);
const { buildContext } = await import(
  join(ROOT, "packages/create-one-terminal/dist/create/context.js")
);

const OUT = mkdtempSync(join(tmpdir(), "ot-scaffold-test-"));
console.log(`Scaffolding to: ${OUT}`);

const ctx = buildContext({
  workspaceName: "test-workspace",
  tauriIdentifier: "com.test.workspace",
  includeFdc3: true,
});

try {
  await renderWorkspace(ctx, OUT);

  const fileCount = execSync(`find "${OUT}" -type f | wc -l`, { encoding: "utf8" }).trim();
  console.log(`✓ Rendered ${fileCount} files`);

  console.log("Running cargo check --workspace…");
  execSync("cargo check --workspace", { cwd: OUT, stdio: "inherit" });
  console.log("✓ cargo check passed");

  console.log("Running npm install…");
  execSync("npm install --ignore-scripts", { cwd: OUT, stdio: "inherit" });
  console.log("✓ npm install passed");

  console.log("Running npm run build:app-directory…");
  execSync("npm run build:app-directory", { cwd: OUT, stdio: "inherit" });
  console.log("✓ build:app-directory passed");

  console.log(`\n✓ All checks passed. Output: ${OUT}`);
} finally {
  if (!keep) {
    rmSync(OUT, { recursive: true, force: true });
    if (!keep) console.log("(temp dir cleaned up — use --keep to retain it)");
  }
}
