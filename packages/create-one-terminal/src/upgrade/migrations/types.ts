export type PatchOperation =
  | { op: "insert-after-line-matching"; pattern: string; content: string }
  | { op: "replace-line-matching"; pattern: string; replacement: string }
  | { op: "add-file"; sourcePath: string; targetPath: string };

export type MigrationSpec =
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
