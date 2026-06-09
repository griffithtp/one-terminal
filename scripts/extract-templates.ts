#!/usr/bin/env tsx
/**
 * Derives EJS templates from the live source apps.
 * Templates are NEVER hand-edited — always re-run this script after modifying apps/.
 *
 * Usage:
 *   npx tsx scripts/extract-templates.ts
 *   npx tsx scripts/extract-templates.ts --output /tmp/extracted-templates
 */

import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { extname, join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { readdir, stat } from "node:fs/promises";

const ROOT = join(fileURLToPath(import.meta.url), "../..");

const outputArg = process.argv.indexOf("--output");
const OUTPUT =
  outputArg !== -1
    ? process.argv[outputArg + 1]
    : join(ROOT, "packages/create-one-terminal/templates");

const manifestArg = process.argv.indexOf("--manifest");
const MANIFEST_PATH =
  manifestArg !== -1
    ? process.argv[manifestArg + 1]
    : join(ROOT, "packages/create-one-terminal/static-manifest.json");

// ── Binary file extensions — copied verbatim ──────────────────────────────────
const BINARY_EXTENSIONS = new Set([".png", ".ico", ".icns", ".svg"]);

// ── Static files — copied verbatim, no .ejs extension ────────────────────────
// Matched against the filename (basename) only
const STATIC_FILENAMES = new Set(["build.rs", "vite-env.d.ts"]);

// ── Directories to skip entirely ──────────────────────────────────────────────
const SKIP_DIRS = new Set(["node_modules", "target", "dist", "gen", ".git"]);

// ── Overlay model ──────────────────────────────────────────────────────────────
// Templates are partitioned across three overlays:
//   shared      — common to all variants (Terminal shell, fdc3 browser/TS, ot-core)
//   enterprise  — full-stack-only (Desktop Agent, App Directory, ot-fdc3, samples)
//   standalone  — slim-variant-only (sample-widget, slim Cargo/package — added in Phase B)
//
// render.ts walks `shared/` then `<variant>/`. variant-overlay-wins on collision.
type Overlay = "shared" | "enterprise" | "standalone";

// ── Source trees to walk ───────────────────────────────────────────────────────
const SOURCES: Array<{ src: string; templateBase: string; overlay: Overlay }> = [
  { src: join(ROOT, "apps/one-terminal"), templateBase: "apps/one-terminal", overlay: "shared" },
  { src: join(ROOT, "packages/ot-core"), templateBase: "packages/ot-core", overlay: "shared" },
  {
    src: join(ROOT, "packages/fdc3-plugin"),
    templateBase: "packages/fdc3-plugin",
    overlay: "shared",
  },

  {
    src: join(ROOT, "apps/desktop-agent"),
    templateBase: "apps/desktop-agent",
    overlay: "enterprise",
  },
  // fdc3-client is Tauri-bound — every method invokes an `fdc3_*` Tauri
  // command registered by ot-fdc3 (Enterprise only). Including it in
  // Standalone would ship dead code that throws on first call.
  {
    src: join(ROOT, "packages/fdc3-client"),
    templateBase: "packages/fdc3-client",
    overlay: "enterprise",
  },
  {
    src: join(ROOT, "apps/tauri-webview-host"),
    templateBase: "apps/tauri-webview-host",
    overlay: "enterprise",
  },
  {
    src: join(ROOT, "apps/app-directory"),
    templateBase: "apps/app-directory",
    overlay: "enterprise",
  },
  {
    src: join(ROOT, "apps/sample-ticker"),
    templateBase: "apps/sample-ticker",
    overlay: "enterprise",
  },
  {
    src: join(ROOT, "apps/sample-chart"),
    templateBase: "apps/sample-chart",
    overlay: "enterprise",
  },
  {
    src: join(ROOT, "apps/electron-host"),
    templateBase: "apps/electron-host",
    overlay: "enterprise",
  },
  { src: join(ROOT, "packages/ot-fdc3"), templateBase: "packages/ot-fdc3", overlay: "enterprise" },

  // Standalone-only sources
  {
    src: join(ROOT, "apps/sample-widget"),
    templateBase: "apps/sample-widget",
    overlay: "standalone",
  },
];

// ── Overlay override directories ──────────────────────────────────────────────
// Hand-authored files (.ejs templates and static files) that don't have a live
// source equivalent. Used for variant-specific root files like the slim
// standalone Cargo.toml / package.json. Copied verbatim — no substitutions.
const OVERLAY_OVERRIDES: Array<{ src: string; overlay: Overlay }> = [
  {
    src: join(ROOT, "packages/create-one-terminal/overlay-overrides/shared"),
    overlay: "shared",
  },
  {
    src: join(ROOT, "packages/create-one-terminal/overlay-overrides/standalone"),
    overlay: "standalone",
  },
];

// ── Scripts to strip from the root package.json template ─────────────────────
// These are framework development scripts — not applicable to scaffolded workspaces.
const ROOT_PACKAGE_SCRIPTS_OMIT = new Set([
  "extract-templates",
  "check-template-drift",
  "create-migration",
  "build:scaffolder",
  "scaffold",
  "test:scaffold",
  // sample-widget is standalone-only; its dev script lives in the
  // standalone overlay-overrides package.json, not in the extracted one.
  "dev:sample-widget",
]);

// Root-level files that also become templates.
// In Phase A these are enterprise-only; Phase B will add standalone variants.
const ROOT_FILES: Array<{ src: string; templateBase: string; overlay: Overlay }> = [
  { src: join(ROOT, "Cargo.toml"), templateBase: "Cargo.toml", overlay: "enterprise" },
  { src: join(ROOT, "package.json"), templateBase: "package.json", overlay: "enterprise" },
  { src: join(ROOT, ".gitignore"), templateBase: ".gitignore", overlay: "shared" },
];

// ── Substitution table (applied in order — longest/most-specific first) ──────
// Each entry: [literal string in source, EJS replacement]
const SUBSTITUTIONS: Array<[string, string]> = [
  // Tauri identifiers (most-specific first)
  ["com.one-terminal.one-terminal", "<%= tauriIdentifier %>.terminal"],
  ["com.one-terminal.desktop-agent", "<%= tauriIdentifier %>.agent"],
  ["com.one-terminal", "<%= tauriIdentifier %>"],

  // npm scoped package names (specific before catch-all)
  ["@one-terminal/one-terminal", "@<%= orgScope %>/one-terminal"],
  ["@one-terminal/desktop-agent", "@<%= orgScope %>/desktop-agent"],
  ["@one-terminal/fdc3-client", "@<%= orgScope %>/fdc3-client"],
  ["@one-terminal/fdc3-plugin", "@<%= orgScope %>/fdc3-plugin"],
  ["@one-terminal/app-directory", "@<%= orgScope %>/app-directory"],
  ["@one-terminal/sample-ticker", "@<%= orgScope %>/sample-ticker"],
  ["@one-terminal/sample-chart", "@<%= orgScope %>/sample-chart"],
  ["@one-terminal/electron-host", "@<%= orgScope %>/electron-host"],
  ["@one-terminal/tauri-webview-host", "@<%= orgScope %>/tauri-webview-host"],
  ["@one-terminal/", "@<%= orgScope %>/"],

  // Root workspace name in package.json (whole-field match)
  ['"name": "one-terminal"', '"name": "<%= workspaceName %>"'],

  // Tauri productName
  ['"productName": "one-terminal"', '"productName": "<%= workspaceName %>-terminal"'],
  ['"productName": "desktop-agent"', '"productName": "<%= workspaceName %>-agent"'],

  // Display name strings
  ['"title": "OneTerminal Desktop Agent"', '"title": "<%= displayName %> Desktop Agent"'],
  ['"title": "OneTerminal"', '"title": "<%= displayName %>"'],
  ["OneTerminal Desktop Agent", "<%= displayName %> Desktop Agent"],
  ["One Terminal", "<%= displayName %>"],
  ["Central Desktop Agent", "<%= displayName %> Desktop Agent"],

  // Rust lib function calls and names
  ["one_terminal_lib::run()", "<%= snakeWorkspaceName %>_terminal_lib::run()"],
  ["desktop_agent_lib::run()", "<%= snakeWorkspaceName %>_agent_lib::run()"],
  ['name = "one_terminal_lib"', 'name = "<%= snakeWorkspaceName %>_terminal_lib"'],
  ['name = "desktop_agent_lib"', 'name = "<%= snakeWorkspaceName %>_agent_lib"'],

  // Dev server ports — URLs first, then bare port values
  ["http://localhost:1422", "http://localhost:<%= terminalDevPort %>"],
  ["http://localhost:1421", "http://localhost:<%= agentDevPort %>"],
  ["port: 1422", "port: <%= terminalDevPort %>"],
  ["port: 1421", "port: <%= agentDevPort %>"],
  ['"port": 1422', '"port": <%= terminalDevPort %>'],
  ['"port": 1421', '"port": <%= agentDevPort %>'],

  // vite --port flag in package.json scripts
  ["vite --port 1422", "vite --port <%= terminalDevPort %>"],
  ["vite --port 1421", "vite --port <%= agentDevPort %>"],

  // Broker / bus / dacp ports
  ['"tcpBroker": 7890', '"tcpBroker": <%= tcpBrokerPort %>'],
  ['"fdc3Bus": 7891', '"fdc3Bus": <%= fdc3BusPort %>'],
  ['"dacpBridge": 4475', '"dacpBridge": <%= dacpBridgePort %>'],

  // App Directory URLs
  [
    '"appDirectoryUrl": "http://localhost:3005"',
    '"appDirectoryUrl": "http://localhost:<%= appDirectoryPort %>"',
  ],
  [
    '"engineCatalogUrl": "http://localhost:3005"',
    '"engineCatalogUrl": "http://localhost:<%= appDirectoryPort %>"',
  ],

  // npm workspace commands in tauri.conf.json
  ["npm -w @one-terminal/one-terminal", "npm -w @<%= orgScope %>/one-terminal"],
  ["npm -w @one-terminal/desktop-agent", "npm -w @<%= orgScope %>/desktop-agent"],
];

// ── ot:if conditional pattern ─────────────────────────────────────────────────
// Matches: `  some content  # ot:if varName` or `  some content  // ot:if varName`
const OT_IF_PATTERN = /^(.*?)\s*(?:#|\/\/)\s*ot:if\s+(\w+)\s*$/;

// ── Dynamic detection ─────────────────────────────────────────────────────────
// A file is dynamic if extract-templates inserted a known context variable
// reference. Checking for the variable names (rather than bare `<%`) prevents
// false positives from source files that might coincidentally contain `<%`.
const CONTEXT_VARS = [
  "workspaceName",
  "orgScope",
  "tauriIdentifier",
  "displayName",
  "snakeWorkspaceName",
  "terminalDevPort",
  "agentDevPort",
  "tcpBrokerPort",
  "fdc3BusPort",
  "dacpBridgePort",
  "appDirectoryPort",
  "includeFdc3",
  "variant",
  "externalFdc3AgentUrl",
  "scaffoldVersion",
  "scaffoldedAt",
];
const DYNAMIC_PATTERN = new RegExp(`<%[\\s\\-=]*(?:if\\s*\\(\\s*)?(?:${CONTEXT_VARS.join("|")})`);

function isDynamic(content: string): boolean {
  return DYNAMIC_PATTERN.test(content);
}

// Collected during processing; written to MANIFEST_PATH at the end.
// Each entry pairs the source path (relative to ROOT) with the overlay it
// belongs to. resolve-static-manifest copies <ROOT>/<path> → <dist/templates>/<overlay>/<path>.
interface StaticEntry {
  path: string;
  overlay: Overlay;
}
const staticEntries: StaticEntry[] = [];

// ── Main ──────────────────────────────────────────────────────────────────────
console.log(`Extracting templates to: ${OUTPUT}`);

// Clear existing templates
await rm(OUTPUT, { recursive: true, force: true });
await mkdir(OUTPUT, { recursive: true });

// Process root files
for (const { src, templateBase, overlay } of ROOT_FILES) {
  await processFile(src, join(OUTPUT, overlay, templateBase), overlay);
}

// Process source trees
for (const { src, templateBase, overlay } of SOURCES) {
  await walkAndProcess(src, join(OUTPUT, overlay, templateBase), overlay);
}

// Copy overlay-override directories verbatim (no substitutions). These are
// hand-authored EJS/static files that don't have a live source equivalent.
for (const { src, overlay } of OVERLAY_OVERRIDES) {
  await copyOverlayOverrides(src, join(OUTPUT, overlay));
}

// Sort manifest entries for deterministic output (avoid spurious drift)
staticEntries.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

await mkdir(dirname(MANIFEST_PATH), { recursive: true });
await writeFile(MANIFEST_PATH, JSON.stringify({ static: staticEntries }, null, 2) + "\n", "utf8");

console.log(`Done. ${staticEntries.length} static files recorded in manifest.`);

// ── Helpers ───────────────────────────────────────────────────────────────────

async function copyOverlayOverrides(srcDir: string, destDir: string): Promise<void> {
  let items: string[];
  try {
    items = await readdir(srcDir);
  } catch {
    return; // override dir doesn't exist — skip silently
  }

  for (const item of items) {
    if (SKIP_DIRS.has(item)) continue;
    const srcPath = join(srcDir, item);
    const destPath = join(destDir, item);
    const info = await stat(srcPath);
    if (info.isDirectory()) {
      await copyOverlayOverrides(srcPath, destPath);
    } else {
      await mkdir(dirname(destPath), { recursive: true });
      await copyFile(srcPath, destPath);
    }
  }
}

async function walkAndProcess(srcDir: string, destDir: string, overlay: Overlay): Promise<void> {
  let items: string[];
  try {
    items = await readdir(srcDir);
  } catch {
    return; // source dir doesn't exist — skip silently
  }

  for (const item of items) {
    if (SKIP_DIRS.has(item)) {
      // Carve-out: a `dist/` directory containing only an `index.html` stub
      // is the frontendDist placeholder for tauri-webview-host (and any other
      // headless Tauri host that lives at a URL). Keep walking it so the stub
      // ships with the scaffold. Any larger `dist/` is a build artifact and
      // stays skipped.
      const srcPath = join(srcDir, item);
      const info = await stat(srcPath);
      if (info.isDirectory()) {
        const children = await readdir(srcPath);
        if (children.length === 1 && children[0] === "index.html") {
          await walkAndProcess(srcPath, join(destDir, item), overlay);
        }
      }
      continue;
    }
    const srcPath = join(srcDir, item);
    const destPath = join(destDir, item);
    const info = await stat(srcPath);
    if (info.isDirectory()) {
      await walkAndProcess(srcPath, destPath, overlay);
    } else {
      await processFile(srcPath, destPath, overlay);
    }
  }
}

async function processFile(src: string, destWithoutEjs: string, overlay: Overlay): Promise<void> {
  const ext = extname(src);
  const base = src.split("/").pop()!;

  if (BINARY_EXTENSIONS.has(ext)) {
    await mkdir(dirname(destWithoutEjs), { recursive: true });
    await copyFile(src, destWithoutEjs);
    return;
  }

  if (STATIC_FILENAMES.has(base)) {
    await mkdir(dirname(destWithoutEjs), { recursive: true });
    await copyFile(src, destWithoutEjs);
    return;
  }

  // Text file — apply substitutions and ot:if processing
  let content: string;
  try {
    content = await readFile(src, "utf8");
  } catch {
    return;
  }

  // Strip framework-internal scripts from the root package.json before templating
  if (src === join(ROOT, "package.json")) {
    const parsed = JSON.parse(content);
    for (const key of ROOT_PACKAGE_SCRIPTS_OMIT) {
      delete parsed.scripts?.[key];
    }
    content = JSON.stringify(parsed, null, 2) + "\n";
  }

  content = applySubstitutions(content);
  content = applyConditionals(content);

  if (isDynamic(content)) {
    const dest = destWithoutEjs + ".ejs";
    await mkdir(dirname(dest), { recursive: true });
    await writeFile(dest, content, "utf8");
  } else {
    staticEntries.push({ path: relative(ROOT, src), overlay });
  }
}

function applySubstitutions(content: string): string {
  for (const [literal, replacement] of SUBSTITUTIONS) {
    content = content.split(literal).join(replacement);
  }
  return content;
}

function applyConditionals(content: string): string {
  const lines = content.split("\n");
  const result: string[] = [];

  for (const line of lines) {
    const match = OT_IF_PATTERN.exec(line);
    if (match) {
      const [, body, varName] = match;
      result.push(`<% if (${varName}) { %>`);
      result.push(body);
      result.push("<% } %>");
    } else {
      result.push(line);
    }
  }

  return result.join("\n");
}
