import ejs from "ejs";
import { copyFile, mkdir, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { walk } from "../utils/fs.js";
import type { ScaffoldContext, Variant } from "./context.js";

const BINARY_EXTENSIONS = new Set([".png", ".ico", ".icns", ".svg"]);

const STATIC_FILENAMES = new Set(["build.rs", "vite-env.d.ts"]);

const SKIP_DIRS = ["node_modules", "target", "dist", "gen", ".git"];

// Overlay walk order: shared first, then variant. Same destination wins from
// the later overlay (variant-overlay-wins on collision).
const OVERLAYS_FOR: Record<Variant, ReadonlyArray<"shared" | Variant>> = {
  standalone: ["shared", "standalone"],
  enterprise: ["shared", "enterprise"],
};

interface PendingWrite {
  dest: string;
  content: string | null;
  srcBinary?: string;
}

export async function renderWorkspace(ctx: ScaffoldContext, outputDir: string): Promise<void> {
  const templatesRoot = join(fileURLToPath(import.meta.url), "../..", "templates");

  // Key writes by destination so a later overlay overrides an earlier one.
  const pending = new Map<string, PendingWrite>();

  for (const overlay of OVERLAYS_FOR[ctx.variant]) {
    const overlayDir = join(templatesRoot, overlay);
    const entries = await walk(overlayDir, SKIP_DIRS).catch(() => []);

    for (const { absolute: src, relative } of entries) {
      const destRelative = stripEjsExtension(relative);
      const dest = join(outputDir, destRelative);
      const base = basename(destRelative);
      const ext = extname(destRelative);

      if (BINARY_EXTENSIONS.has(ext) || STATIC_FILENAMES.has(base)) {
        pending.set(dest, { dest, content: null, srcBinary: src });
      } else if (src.endsWith(".ejs")) {
        const rendered = await ejs.renderFile(src, ctx, { async: true });
        pending.set(dest, { dest, content: rendered });
      } else {
        pending.set(dest, { dest, content: null, srcBinary: src });
      }
    }
  }

  for (const w of pending.values()) {
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
