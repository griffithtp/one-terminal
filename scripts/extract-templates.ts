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
import { extname, join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { readdir, stat } from "node:fs/promises";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const outputArg = process.argv.indexOf("--output");
const OUTPUT =
  outputArg !== -1
    ? process.argv[outputArg + 1]
    : join(ROOT, "packages/create-one-terminal/templates");

// ── Binary file extensions — copied verbatim ──────────────────────────────────
const BINARY_EXTENSIONS = new Set([".png", ".ico", ".icns", ".svg"]);

// ── Static files — copied verbatim, no .ejs extension ────────────────────────
// Matched against the filename (basename) only
const STATIC_FILENAMES = new Set(["build.rs", "vite-env.d.ts"]);

// ── Directories to skip entirely ──────────────────────────────────────────────
const SKIP_DIRS = new Set(["node_modules", "target", "dist", "gen", ".git"]);

// ── Source trees to walk ───────────────────────────────────────────────────────
const SOURCES: Array<{ src: string; templateBase: string }> = [
  { src: join(ROOT, "apps/one-terminal"),        templateBase: "apps/one-terminal" },
  { src: join(ROOT, "apps/desktop-agent"),       templateBase: "apps/desktop-agent" },
  { src: join(ROOT, "apps/tauri-webview-host"),  templateBase: "apps/tauri-webview-host" },
  { src: join(ROOT, "packages/ot-core"),    templateBase: "packages/ot-core" },
  { src: join(ROOT, "packages/ot-fdc3"),    templateBase: "packages/ot-fdc3" },
  { src: join(ROOT, "packages/fdc3-plugin"),templateBase: "packages/fdc3-plugin" },
  { src: join(ROOT, "packages/fdc3-client"),templateBase: "packages/fdc3-client" },
];

// Root-level files that also become templates
const ROOT_FILES = [
  { src: join(ROOT, "Cargo.toml"),    templateBase: "Cargo.toml" },
  { src: join(ROOT, "package.json"),  templateBase: "package.json" },
  { src: join(ROOT, ".gitignore"),    templateBase: ".gitignore" },
];

// ── Substitution table (applied in order — longest/most-specific first) ──────
// Each entry: [literal string in source, EJS replacement]
const SUBSTITUTIONS: Array<[string, string]> = [
  // Tauri identifiers (most-specific first)
  ["com.one-terminal.one-terminal",  "<%= tauriIdentifier %>.terminal"],
  ["com.one-terminal.desktop-agent", "<%= tauriIdentifier %>.agent"],
  ["com.one-terminal",               "<%= tauriIdentifier %>"],

  // npm scoped package names (specific before catch-all)
  ["@one-terminal/one-terminal",     "@<%= orgScope %>/one-terminal"],
  ["@one-terminal/desktop-agent",    "@<%= orgScope %>/desktop-agent"],
  ["@one-terminal/fdc3-client",      "@<%= orgScope %>/fdc3-client"],
  ["@one-terminal/fdc3-plugin",      "@<%= orgScope %>/fdc3-plugin"],
  ["@one-terminal/app-directory",    "@<%= orgScope %>/app-directory"],
  ["@one-terminal/sample-ticker",    "@<%= orgScope %>/sample-ticker"],
  ["@one-terminal/sample-chart-viewer", "@<%= orgScope %>/sample-chart-viewer"],
  ["@one-terminal/electron-host",    "@<%= orgScope %>/electron-host"],
  ["@one-terminal/tauri-webview-host","@<%= orgScope %>/tauri-webview-host"],
  ["@one-terminal/",                 "@<%= orgScope %>/"],

  // Root workspace name in package.json (whole-field match)
  ['"name": "one-terminal"',         '"name": "<%= workspaceName %>"'],

  // Tauri productName
  ['"productName": "one-terminal"',  '"productName": "<%= workspaceName %>-terminal"'],
  ['"productName": "desktop-agent"', '"productName": "<%= workspaceName %>-agent"'],

  // Display name strings
  ['"title": "OneTerminal Desktop Agent"', '"title": "<%= displayName %> Desktop Agent"'],
  ['"title": "OneTerminal"',          '"title": "<%= displayName %>"'],
  ["OneTerminal Desktop Agent",       "<%= displayName %> Desktop Agent"],
  ["One Terminal",                    "<%= displayName %>"],
  ["Central Desktop Agent",           "<%= displayName %> Desktop Agent"],

  // Rust lib function calls and names
  ["one_terminal_lib::run()",        "<%= snakeWorkspaceName %>_terminal_lib::run()"],
  ["desktop_agent_lib::run()",       "<%= snakeWorkspaceName %>_agent_lib::run()"],
  ['name = "one_terminal_lib"',      'name = "<%= snakeWorkspaceName %>_terminal_lib"'],
  ['name = "desktop_agent_lib"',     'name = "<%= snakeWorkspaceName %>_agent_lib"'],

  // Dev server ports — URLs first, then bare port values
  ["http://localhost:1422",          "http://localhost:<%= terminalDevPort %>"],
  ["http://localhost:1421",          "http://localhost:<%= agentDevPort %>"],
  ["port: 1422",                     "port: <%= terminalDevPort %>"],
  ["port: 1421",                     "port: <%= agentDevPort %>"],
  ['"port": 1422',                   '"port": <%= terminalDevPort %>'],
  ['"port": 1421',                   '"port": <%= agentDevPort %>'],

  // vite --port flag in package.json scripts
  ["vite --port 1422",               "vite --port <%= terminalDevPort %>"],
  ["vite --port 1421",               "vite --port <%= agentDevPort %>"],

  // Broker / bus / dacp ports
  ['"tcpBroker": 7890',              '"tcpBroker": <%= tcpBrokerPort %>'],
  ['"fdc3Bus": 7891',                '"fdc3Bus": <%= fdc3BusPort %>'],
  ['"dacpBridge": 4475',             '"dacpBridge": <%= dacpBridgePort %>'],

  // App Directory URLs
  ['"appDirectoryUrl": "http://localhost:3005"', '"appDirectoryUrl": "http://localhost:<%= appDirectoryPort %>"'],
  ['"engineCatalogUrl": "http://localhost:3005"', '"engineCatalogUrl": "http://localhost:<%= appDirectoryPort %>"'],

  // npm workspace commands in tauri.conf.json
  ["npm -w @one-terminal/one-terminal",  "npm -w @<%= orgScope %>/one-terminal"],
  ["npm -w @one-terminal/desktop-agent", "npm -w @<%= orgScope %>/desktop-agent"],
];

// ── ot:if conditional pattern ─────────────────────────────────────────────────
// Matches: `  some content  # ot:if varName` or `  some content  // ot:if varName`
const OT_IF_PATTERN = /^(.*?)\s*(?:#|\/\/)\s*ot:if\s+(\w+)\s*$/;

// ── Main ──────────────────────────────────────────────────────────────────────
console.log(`Extracting templates to: ${OUTPUT}`);

// Clear existing templates
await rm(OUTPUT, { recursive: true, force: true });
await mkdir(OUTPUT, { recursive: true });

// Process root files
for (const { src, templateBase } of ROOT_FILES) {
  await processFile(src, join(OUTPUT, templateBase));
}

// Process source trees
for (const { src, templateBase } of SOURCES) {
  await walkAndProcess(src, join(OUTPUT, templateBase));
}

console.log("Done.");

// ── Helpers ───────────────────────────────────────────────────────────────────

async function walkAndProcess(srcDir: string, destDir: string): Promise<void> {
  let items: string[];
  try {
    items = await readdir(srcDir);
  } catch {
    return; // source dir doesn't exist — skip silently
  }

  for (const item of items) {
    if (SKIP_DIRS.has(item)) continue;
    const srcPath = join(srcDir, item);
    const destPath = join(destDir, item);
    const info = await stat(srcPath);
    if (info.isDirectory()) {
      await walkAndProcess(srcPath, destPath);
    } else {
      await processFile(srcPath, destPath);
    }
  }
}

async function processFile(src: string, destWithoutEjs: string): Promise<void> {
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

  content = applySubstitutions(content);
  content = applyConditionals(content);

  const dest = destWithoutEjs + ".ejs";
  await mkdir(dirname(dest), { recursive: true });
  await writeFile(dest, content, "utf8");
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
