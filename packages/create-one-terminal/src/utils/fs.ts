import { readFile, writeFile, mkdir, copyFile, readdir, stat } from "node:fs/promises";
import { join } from "node:path";

export async function readUtf8(path: string): Promise<string> {
  return readFile(path, "utf8");
}

export async function writeUtf8(path: string, content: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content, "utf8");
}

export async function ensureDir(path: string): Promise<void> {
  await mkdir(path, { recursive: true });
}

export async function copyBinary(src: string, dest: string): Promise<void> {
  await mkdir(dirname(dest), { recursive: true });
  await copyFile(src, dest);
}

export interface WalkEntry {
  absolute: string;
  relative: string;
}

export async function walk(root: string, skip: string[] = []): Promise<WalkEntry[]> {
  const entries: WalkEntry[] = [];
  await walkDir(root, root, skip, entries);
  return entries;
}

async function walkDir(root: string, dir: string, skip: string[], out: WalkEntry[]): Promise<void> {
  const items = await readdir(dir);
  for (const item of items) {
    const absolute = join(dir, item);
    const relative = absolute.slice(root.length + 1);
    if (skip.some((s) => relative === s || relative.startsWith(s + "/"))) continue;
    const info = await stat(absolute);
    if (info.isDirectory()) {
      await walkDir(root, absolute, skip, out);
    } else {
      out.push({ absolute, relative });
    }
  }
}

function dirname(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx === -1 ? "." : path.slice(0, idx);
}
