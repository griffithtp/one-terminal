#!/usr/bin/env tsx
/**
 * CI drift check: runs extract-templates into a temp dir and diffs against committed templates.
 * Exits 1 if any drift is detected.
 */

import { execSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const COMMITTED = join(ROOT, "packages/create-one-terminal/templates");
const TMP = mkdtempSync(join(tmpdir(), "ot-templates-"));

try {
  console.log("Extracting templates to temp dir…");
  execSync(`npx tsx ${join(ROOT, "scripts/extract-templates.ts")} --output ${TMP}`, {
    stdio: "inherit",
    cwd: ROOT,
  });

  console.log("Diffing against committed templates…");
  try {
    execSync(`diff -rq "${TMP}" "${COMMITTED}"`, { stdio: "pipe" });
    console.log("✓ Templates are in sync.");
  } catch {
    // diff exits non-zero when files differ
    const diffOutput = execSync(`diff -r "${TMP}" "${COMMITTED}" || true`, {
      encoding: "utf8",
      cwd: ROOT,
    });
    console.error("\n── Template drift detected ──────────────────────────────────\n");
    console.error(diffOutput);
    console.error(
      "Templates are out of sync with the source apps.\n" +
        "Run `npm run extract-templates` and commit the result.\n"
    );
    process.exit(1);
  }
} finally {
  rmSync(TMP, { recursive: true, force: true });
}
