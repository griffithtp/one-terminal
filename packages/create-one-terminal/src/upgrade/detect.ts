import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";

export type Variant = "standalone" | "enterprise";

export interface ProjectMeta {
  version: string;
  scaffoldedAt: string;
  variant: Variant;
}

export async function detectProject(cwd: string): Promise<ProjectMeta> {
  const pkgPath = join(cwd, "package.json");
  let raw: string;
  try {
    raw = await readFile(pkgPath, "utf8");
  } catch {
    throw new Error(
      `No package.json found in ${cwd}. Run this command from your OneTerminal workspace root.`
    );
  }

  const pkg = JSON.parse(raw) as Record<string, unknown>;
  const meta = pkg["oneTerminal"] as Record<string, string> | undefined;
  if (!meta?.version) {
    throw new Error(
      "package.json is missing the oneTerminal.version field.\n" +
        "This does not appear to be a OneTerminal scaffolded project."
    );
  }

  let variant: Variant;
  if (meta.variant === "standalone" || meta.variant === "enterprise") {
    variant = meta.variant;
  } else {
    // Pre-variant scaffolds: infer from the presence of apps/desktop-agent.
    variant = (await dirExists(join(cwd, "apps/desktop-agent"))) ? "enterprise" : "standalone";
  }

  return {
    version: meta.version,
    scaffoldedAt: meta.scaffoldedAt ?? "unknown",
    variant,
  };
}

async function dirExists(path: string): Promise<boolean> {
  try {
    const info = await stat(path);
    return info.isDirectory();
  } catch {
    return false;
  }
}
