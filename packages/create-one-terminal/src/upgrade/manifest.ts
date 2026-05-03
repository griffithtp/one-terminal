import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { VersionsManifest } from "./migrations/types.js";

const BUNDLED_PATH = join(
  fileURLToPath(import.meta.url),
  "../../../../versions.json",
);

export async function fetchManifest(): Promise<VersionsManifest> {
  // Try npm registry for latest published manifest
  try {
    const res = await fetch("https://registry.npmjs.org/create-one-terminal/latest", {
      signal: AbortSignal.timeout(4000),
    });
    if (res.ok) {
      // The manifest is bundled in the package; use the dist-tags.latest version
      // as a version signal only — fall back to bundled for the actual migration data
      // since we can't fetch arbitrary file content from the registry directly.
      await res.json(); // consume body
    }
  } catch {
    // network unavailable — use bundled
  }

  return readBundledManifest();
}

async function readBundledManifest(): Promise<VersionsManifest> {
  const raw = await readFile(BUNDLED_PATH, "utf8");
  return JSON.parse(raw) as VersionsManifest;
}
