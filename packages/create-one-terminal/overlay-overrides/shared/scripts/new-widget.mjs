#!/usr/bin/env node
/**
 * scripts/new-widget.mjs
 *
 * Self-contained widget generator for OneTerminal workspaces. No external
 * dependencies — uses only the Node.js standard library. Reads the workspace
 * variant from package.json's `oneTerminal.variant` and registers the new
 * widget in widgets.config.json (Standalone) or prints a curl POST /v2/apps
 * payload (Enterprise).
 *
 * Usage:
 *   npm run create-widget                                          # interactive
 *   npm run create-widget -- --name fx-rates                       # name as flag
 *   npm run create-widget -- --name fx-rates --title "FX Rates" --port 3010
 *                                                                  # fully scripted
 *
 * The widget template is bundled alongside this file under
 * scripts/widget-template/. Edit the templates to change what new widgets
 * look like — the change applies on the next run.
 */

import { createInterface } from "node:readline/promises";
import { stdin as input, stdout as output } from "node:process";
import { access, mkdir, readdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const WORKSPACE_ROOT = join(SCRIPT_DIR, "..");
const TEMPLATE_DIR = join(SCRIPT_DIR, "widget-template");

const RESERVED = new Set([
  "one-terminal",
  "desktop-agent",
  "app-directory",
  "tauri-webview-host",
  "electron-host",
  "ot-core",
  "ot-fdc3",
  "fdc3-plugin",
  "fdc3-client",
]);

const args = parseArgs(process.argv.slice(2));
let rl = null;

main().then(
  () => rl?.close(),
  (err) => {
    rl?.close();
    console.error(`\n✗ ${err.message}`);
    process.exit(1);
  }
);

async function main() {
  const project = await detectProject();

  const widgetName = args.name ?? (await promptName());
  validateName(widgetName);

  const widgetDir = join(WORKSPACE_ROOT, "apps", widgetName);
  if (await pathExists(widgetDir)) {
    fail(`apps/${widgetName} already exists`);
  }

  const widgetTitle =
    args.title ?? (await prompt("Widget title (display name)", toTitleCase(widgetName)));

  const suggestedPort = await suggestPort();
  const widgetPort =
    args.port ?? Number(await prompt("Dev server port", String(suggestedPort), validatePort));

  const orgScope = await detectOrgScope();

  const spec = { widgetName, widgetTitle, widgetPort, orgScope };

  console.log(`\nScaffolding apps/${widgetName}/  (variant: ${project.variant})`);
  await renderTemplate(spec);
  await register(spec, project.variant);

  console.log(`\n✓ Created apps/${widgetName}/`);
  console.log("\nNext steps:");
  console.log("  npm install");
  console.log(`  npm run dev --workspace apps/${widgetName}`);
  console.log("  # restart the Terminal to see the widget in the launcher");
}

// ── Args ─────────────────────────────────────────────────────────────────────

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--name") out.name = argv[++i];
    else if (a === "--title") out.title = argv[++i];
    else if (a === "--port") out.port = Number(argv[++i]);
    else if (!a.startsWith("--") && !out.name) out.name = a;
  }
  return out;
}

// ── Workspace detection ──────────────────────────────────────────────────────

async function detectProject() {
  const pkgPath = join(WORKSPACE_ROOT, "package.json");
  let pkg;
  try {
    pkg = JSON.parse(await readFile(pkgPath, "utf8"));
  } catch (e) {
    fail(`Could not read ${pkgPath}: ${e.message}`);
  }
  const meta = pkg.oneTerminal;
  if (!meta?.version) {
    fail(
      "package.json is missing the oneTerminal.version field.\n" +
        "This script must be run from the root of a OneTerminal workspace."
    );
  }
  const variant = meta.variant === "standalone" ? "standalone" : "enterprise";
  return { variant, version: meta.version };
}

async function detectOrgScope() {
  try {
    const pkg = JSON.parse(await readFile(join(WORKSPACE_ROOT, "package.json"), "utf8"));
    return typeof pkg.name === "string" && pkg.name ? pkg.name : "workspace";
  } catch {
    return "workspace";
  }
}

async function suggestPort() {
  const used = new Set();
  try {
    const items = await readdir(join(WORKSPACE_ROOT, "apps"));
    for (const item of items) {
      try {
        const text = await readFile(join(WORKSPACE_ROOT, "apps", item, "server.js"), "utf8");
        const m = /(?:const|let|var)\s+PORT\s*=\s*(\d{4,5})/.exec(text);
        if (m) used.add(Number(m[1]));
      } catch {
        // no server.js; ignore
      }
    }
  } catch {
    // no apps/ dir; ignore
  }
  let port = 3010;
  while (used.has(port)) port++;
  return port;
}

// ── Template rendering ───────────────────────────────────────────────────────

async function renderTemplate(spec) {
  const destDir = join(WORKSPACE_ROOT, "apps", spec.widgetName);
  const files = await walkDir(TEMPLATE_DIR);
  for (const { absolute, relative } of files) {
    const raw = await readFile(absolute, "utf8");
    const rendered = raw.replace(/<%=\s*(\w+)\s*%>/g, (_, key) => {
      if (!(key in spec)) {
        throw new Error(`Unknown template variable in ${relative}: ${key}`);
      }
      return String(spec[key]);
    });
    const dest = join(destDir, relative.replace(/\.tpl$/, ""));
    await mkdir(dirname(dest), { recursive: true });
    await writeFile(dest, rendered, "utf8");
  }
}

async function walkDir(dir, base = dir, out = []) {
  for (const item of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, item.name);
    if (item.isDirectory()) {
      await walkDir(full, base, out);
    } else {
      out.push({ absolute: full, relative: full.slice(base.length + 1) });
    }
  }
  return out;
}

// ── Registration ─────────────────────────────────────────────────────────────

async function register(spec, variant) {
  if (variant === "standalone") {
    await appendToWidgetsConfig(spec);
    return;
  }
  await printOrPostAppDirectory(spec);
}

async function appendToWidgetsConfig(spec) {
  const path = join(WORKSPACE_ROOT, "widgets.config.json");
  let cfg;
  try {
    cfg = JSON.parse(await readFile(path, "utf8"));
  } catch {
    cfg = { widgets: [] };
  }
  cfg.widgets = Array.isArray(cfg.widgets) ? cfg.widgets : [];
  cfg.widgets.push({
    appId: spec.widgetName,
    title: spec.widgetTitle,
    url: `http://localhost:${spec.widgetPort}`,
    categories: [],
  });
  await writeFile(path, JSON.stringify(cfg, null, 2) + "\n", "utf8");
  console.log("✓ Updated widgets.config.json");
}

async function printOrPostAppDirectory(spec) {
  const url = process.env.OT_APP_DIR_URL ?? "http://localhost:3005/v2/apps";
  const payload = {
    appId: spec.widgetName,
    name: spec.widgetName,
    title: spec.widgetTitle,
    type: "web",
    details: { url: `http://localhost:${spec.widgetPort}` },
    categories: [],
  };

  const token = process.env.OT_APPD_TOKEN;
  if (token) {
    const ok = await tryPost(url, payload, token);
    if (ok) {
      console.log(`✓ Registered with App Directory at ${url}`);
      return;
    }
  }
  console.log("\nRegister the widget with your App Directory:");
  console.log(`  curl -X POST ${url} \\`);
  console.log(`    -H 'Content-Type: application/json' \\`);
  if (token) console.log("    -H 'Authorization: Bearer $OT_APPD_TOKEN' \\");
  console.log(`    -d '${JSON.stringify(payload)}'`);
}

async function tryPost(url, payload, token) {
  try {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify(payload),
    });
    return res.ok;
  } catch {
    return false;
  }
}

// ── Prompts ──────────────────────────────────────────────────────────────────

async function promptName() {
  while (true) {
    const v = (await prompt("Widget name (kebab-case)", "")).trim();
    try {
      validateName(v);
      return v;
    } catch (e) {
      console.error(`✗ ${e.message}`);
    }
  }
}

async function prompt(message, defaultValue, validate) {
  rl ??= createInterface({ input, output });
  const hint = defaultValue ? ` [${defaultValue}]` : "";
  while (true) {
    const raw = await rl.question(`${message}${hint}: `);
    const v = raw.trim() || defaultValue;
    if (!validate) return v;
    const err = validate(v);
    if (!err) return v;
    console.error(`✗ ${err}`);
  }
}

function validatePort(v) {
  const n = Number(v);
  if (!Number.isInteger(n) || n < 1024 || n > 65535)
    return "Must be an integer between 1024 and 65535";
}

function validateName(v) {
  if (!/^[a-z][a-z0-9-]+$/.test(v)) throw new Error("Must be lowercase kebab-case (e.g. fx-rates)");
  if (RESERVED.has(v)) throw new Error(`'${v}' is reserved for framework apps`);
}

// ── Utils ────────────────────────────────────────────────────────────────────

async function pathExists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

function toTitleCase(s) {
  return s
    .split("-")
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join(" ");
}

function fail(msg) {
  console.error(`✗ ${msg}`);
  process.exit(1);
}
