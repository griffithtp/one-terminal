import ejs from "ejs";
import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { walk } from "../utils/fs.js";
import type { ScaffoldContext } from "./context.js";

const BINARY_EXTENSIONS = new Set([".png", ".ico", ".icns", ".svg"]);

const STATIC_FILENAMES = new Set(["build.rs", "vite-env.d.ts"]);

const SKIP_DIRS = ["node_modules", "target", "dist", "gen", ".git"];

export async function renderWorkspace(ctx: ScaffoldContext, outputDir: string): Promise<void> {
  const templatesDir = join(fileURLToPath(import.meta.url), "../..", "templates");
  const entries = await walk(templatesDir, SKIP_DIRS);

  // Collect all renders into memory before writing (atomic: no partial writes on error)
  const writes: Array<{ dest: string; content: string | null; srcBinary?: string }> = [];

  for (const { absolute: src, relative } of entries) {
    const destRelative = stripEjsExtension(relative);
    const dest = join(outputDir, destRelative);
    const base = basename(destRelative);
    const ext = extname(destRelative);

    if (BINARY_EXTENSIONS.has(ext)) {
      writes.push({ dest, content: null, srcBinary: src });
    } else if (STATIC_FILENAMES.has(base)) {
      writes.push({ dest, content: null, srcBinary: src });
    } else if (src.endsWith(".ejs")) {
      const rendered = await ejs.renderFile(src, ctx, { async: true });
      writes.push({ dest, content: rendered });
    } else {
      // non-.ejs text file (e.g. a plain .json or .rs without template vars)
      writes.push({ dest, content: null, srcBinary: src });
    }
  }

  for (const w of writes) {
    await mkdir(dirname(w.dest), { recursive: true });
    if (w.content !== null) {
      await writeFile(w.dest, w.content, "utf8");
    } else {
      await copyFile(w.srcBinary!, w.dest);
    }
  }
}

function stripEjsExtension(relative: string): string {
  return relative.endsWith(".ejs") ? relative.slice(0, -4) : relative;
}

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

function dirname(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx === -1 ? "." : path.slice(0, idx);
}
