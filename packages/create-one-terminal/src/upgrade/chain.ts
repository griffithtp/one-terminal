import { semverGt } from "../utils/semver.js";
import type { Variant } from "./detect.js";
import type { MigrationSpec, VersionEntry, VersionsManifest } from "./migrations/types.js";

export interface MigrationChainEntry {
  version: string;
  migration: MigrationSpec;
}

export function buildChain(
  manifest: VersionsManifest,
  currentVersion: string,
  variant: Variant = "enterprise"
): MigrationChainEntry[] {
  const relevant = manifest.versions
    .filter((v) => semverGt(v.version, currentVersion))
    .sort((a, b) => (semverGt(a.version, b.version) ? 1 : -1));

  return relevant.flatMap((v: VersionEntry) =>
    v.migrations
      .filter((m) => migrationApplies(m, variant))
      .map((m) => ({ version: v.version, migration: m }))
  );
}

function migrationApplies(m: MigrationSpec, variant: Variant): boolean {
  // Default "both" preserves behaviour for pre-variant migrations.
  const applies = m.appliesTo ?? "both";
  return applies === "both" || applies === variant;
}

export function latestVersion(manifest: VersionsManifest): string {
  const sorted = [...manifest.versions].sort((a, b) => (semverGt(a.version, b.version) ? -1 : 1));
  return sorted[0]?.version ?? "0.1.0";
}
