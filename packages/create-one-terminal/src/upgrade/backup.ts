import { copyFile, mkdir, rm } from "node:fs/promises";
import { join, dirname } from "node:path";

export function backupDir(cwd: string, targetVersion: string): string {
  return join(cwd, ".one-terminal", `upgrade-backup-${targetVersion}`);
}

export async function snapshot(
  cwd: string,
  targetVersion: string,
  targets: string[],
): Promise<void> {
  const dir = backupDir(cwd, targetVersion);
  // Fresh snapshot each time
  await rm(dir, { recursive: true, force: true });
  await mkdir(dir, { recursive: true });

  for (const rel of targets) {
    const src = join(cwd, rel);
    const dest = join(dir, rel);
    try {
      await mkdir(dirname(dest), { recursive: true });
      await copyFile(src, dest);
    } catch {
      // file may not exist yet (add-file migrations) — skip
    }
  }
}
