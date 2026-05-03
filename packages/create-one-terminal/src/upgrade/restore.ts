import { copyFile, mkdir } from "node:fs/promises";
import { join, dirname } from "node:path";
import { backupDir } from "./backup.js";
import { walk } from "../utils/fs.js";

export async function restore(cwd: string, targetVersion: string): Promise<void> {
  const dir = backupDir(cwd, targetVersion);
  const entries = await walk(dir).catch(() => []);
  for (const { absolute, relative } of entries) {
    const dest = join(cwd, relative);
    await mkdir(dirname(dest), { recursive: true });
    await copyFile(absolute, dest);
  }
}
