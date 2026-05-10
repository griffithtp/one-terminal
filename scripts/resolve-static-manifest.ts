#!/usr/bin/env tsx
/**
 * Copies static (non-templated) files from live source into dist/templates/
 * using the entries listed in static-manifest.json.
 *
 * Run as part of the build:scaffolder step, after `tsc` and after copying
 * dynamic EJS templates to dist/templates/.
 *
 * Usage:
 *   npx tsx scripts/resolve-static-manifest.ts
 */

import { copyFile, mkdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const MANIFEST_PATH = join(ROOT, "packages/create-one-terminal/static-manifest.json");
const DIST_TEMPLATES = join(ROOT, "packages/create-one-terminal/dist/templates");

const manifest: { static: string[] } = JSON.parse(await readFile(MANIFEST_PATH, "utf8"));

for (const path of manifest.static) {
  const src = join(ROOT, path);
  const dest = join(DIST_TEMPLATES, path);
  await mkdir(dirname(dest), { recursive: true });
  await copyFile(src, dest);
}

console.log(`Resolved ${manifest.static.length} static files into dist/templates.`);
