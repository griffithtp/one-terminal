export type PatchOperation =
  | { op: "insert-after-line-matching"; pattern: string; content: string }
  | { op: "replace-line-matching"; pattern: string; replacement: string }
  | { op: "add-file"; sourcePath: string; targetPath: string };

/**
 * Which workspace variants a migration applies to. Inferred at authoring time
 * from the overlay the changed template lives in:
 *   shared/      → "both"
 *   enterprise/  → "enterprise"
 *   standalone/  → "standalone"
 *
 * Older migrations without this field default to "both" for backward
 * compatibility with pre-variant scaffolds.
 */
export type AppliesTo = "both" | "standalone" | "enterprise";

export type MigrationSpec =
  | {
      type: "config-merge";
      id: string;
      target: string;
      description: string;
      patch: Record<string, unknown>;
      appliesTo?: AppliesTo;
    }
  | {
      type: "dep-bump";
      id: string;
      target: string;
      description: string;
      deps: Array<{ name: string; ecosystem: "cargo" | "npm"; newVersion: string }>;
      appliesTo?: AppliesTo;
    }
  | {
      type: "structural";
      id: string;
      target: string;
      description: string;
      operations: PatchOperation[];
      appliesTo?: AppliesTo;
    };

export type MigrationStatus = "applied" | "skipped" | "needs-manual" | "failed";

export interface MigrationResult {
  id: string;
  description: string;
  status: MigrationStatus;
  detail?: string;
}

export interface VersionEntry {
  version: string;
  releaseDate: string;
  breaking: boolean;
  changelogUrl: string;
  migrations: MigrationSpec[];
}

export interface VersionsManifest {
  schemaVersion: 1;
  versions: VersionEntry[];
}
