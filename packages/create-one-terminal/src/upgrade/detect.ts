import { readFile } from "node:fs/promises";
import { join } from "node:path";

export interface ProjectMeta {
  version: string;
  scaffoldedAt: string;
}

export async function detectProject(cwd: string): Promise<ProjectMeta> {
  const pkgPath = join(cwd, "package.json");
  let raw: string;
  try {
    raw = await readFile(pkgPath, "utf8");
  } catch {
    throw new Error(`No package.json found in ${cwd}. Run this command from your OneTerminal workspace root.`);
  }

  const pkg = JSON.parse(raw) as Record<string, unknown>;
  const meta = pkg["oneTerminal"] as Record<string, string> | undefined;
  if (!meta?.version) {
    throw new Error(
      "package.json is missing the oneTerminal.version field.\n" +
        "This does not appear to be a OneTerminal scaffolded project.",
    );
  }

  return { version: meta.version, scaffoldedAt: meta.scaffoldedAt ?? "unknown" };
}
