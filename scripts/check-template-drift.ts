#!/usr/bin/env tsx
/**
 * CI drift check: runs extract-templates into a temp dir and diffs against committed
 * templates and static-manifest.json. Exits 1 if any drift is detected.
 */

import { execSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const COMMITTED_TEMPLATES = join(ROOT, "packages/create-one-terminal/templates");
const COMMITTED_MANIFEST = join(ROOT, "packages/create-one-terminal/static-manifest.json");

const TMP = mkdtempSync(join(tmpdir(), "ot-templates-"));
const TMP_TEMPLATES = join(TMP, "templates");
const TMP_MANIFEST = join(TMP, "static-manifest.json");

try {
  console.log("Extracting templates to temp dir…");
  execSync(
    `npx tsx ${join(ROOT, "scripts/extract-templates.ts")} --output ${TMP_TEMPLATES} --manifest ${TMP_MANIFEST}`,
    { stdio: "inherit", cwd: ROOT }
  );

  let drifted = false;

  console.log("Diffing dynamic templates…");
  try {
    execSync(`diff -rq "${TMP_TEMPLATES}" "${COMMITTED_TEMPLATES}"`, { stdio: "pipe" });
    console.log("✓ Dynamic templates are in sync.");
  } catch {
    const diffOutput = execSync(`diff -r "${TMP_TEMPLATES}" "${COMMITTED_TEMPLATES}" || true`, {
      encoding: "utf8",
      cwd: ROOT,
    });
    console.error("\n── Dynamic template drift detected ──────────────────────────────────\n");
    console.error(diffOutput);
    drifted = true;
  }

  console.log("Diffing static manifest…");
  try {
    execSync(`diff -q "${TMP_MANIFEST}" "${COMMITTED_MANIFEST}"`, { stdio: "pipe" });
    console.log("✓ Static manifest is in sync.");
  } catch {
    const diffOutput = execSync(`diff "${TMP_MANIFEST}" "${COMMITTED_MANIFEST}" || true`, {
      encoding: "utf8",
      cwd: ROOT,
    });
    console.error("\n── Static manifest drift detected ───────────────────────────────────\n");
    console.error(diffOutput);
    drifted = true;
  }

  if (drifted) {
    console.error(
      "\nTemplates are out of sync with the source apps.\n\n" +
        "To fix, run the following steps:\n\n" +
        "  1. npm run create-migration   # recommended: extracts templates, authors\n" +
        "                                #   migrations, and bumps the version\n" +
        "     — OR —\n" +
        "     npm run extract-templates  # re-extract templates only (no migration)\n\n" +
        "  2. npm run build:scaffolder   # recompile the scaffolder\n\n" +
        "  3. git add packages/create-one-terminal/ && git commit\n"
    );
    process.exit(1);
  }
} finally {
  rmSync(TMP, { recursive: true, force: true });
}
