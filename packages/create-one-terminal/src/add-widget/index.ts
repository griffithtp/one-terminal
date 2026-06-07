import * as p from "@clack/prompts";
import ejs from "ejs";
import { readFile, readdir, writeFile, mkdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { walk } from "../utils/fs.js";
import { detectProject, type Variant } from "../upgrade/detect.js";

const TEMPLATE_DIR = join(fileURLToPath(import.meta.url), "../..", "widget-template");
const RESERVED_NAMES = new Set([
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

export interface WidgetSpec {
  widgetName: string;
  widgetTitle: string;
  widgetPort: number;
  orgScope: string;
}

/**
 * Programmatic equivalent of `runAddWidget` — no prompts. Scaffolds the
 * widget files and registers it for the workspace variant. Test scripts and
 * future automation use this entry point.
 */
export async function createWidget(cwd: string, spec: WidgetSpec): Promise<void> {
  const project = await detectProject(cwd);
  const appsDir = join(cwd, "apps");
  if (await dirExists(join(appsDir, spec.widgetName))) {
    throw new Error(`apps/${spec.widgetName} already exists`);
  }
  await renderWidgetFiles(spec, appsDir);
  await registerWidget(spec, cwd, project.variant);
}

export async function runAddWidget(cwd = process.cwd()): Promise<void> {
  p.intro("OneTerminal · add-widget");

  const project = await detectProject(cwd).catch((err: Error) => {
    p.cancel(err.message);
    process.exit(1);
  });

  const widgetName = await p.text({
    message: "Widget name (kebab-case)",
    placeholder: "fx-rates",
    validate: (v) => {
      if (!/^[a-z][a-z0-9-]+$/.test(v)) return "Must be lowercase kebab-case";
      if (RESERVED_NAMES.has(v)) return `${v} is reserved`;
    },
  });
  if (p.isCancel(widgetName)) cancel();

  const appsDir = join(cwd, "apps");
  if (await dirExists(join(appsDir, widgetName as string))) {
    p.cancel(`apps/${widgetName as string} already exists`);
    process.exit(1);
  }

  const widgetTitle = await p.text({
    message: "Widget title (display name)",
    placeholder: (widgetName as string).replace(/-/g, " "),
    defaultValue: toTitleCase(widgetName as string),
    validate: (v) => {
      if (!v.trim()) return "Title cannot be empty";
    },
  });
  if (p.isCancel(widgetTitle)) cancel();

  const suggestedPort = await suggestPort(appsDir);
  const widgetPortStr = await p.text({
    message: "Dev server port",
    initialValue: String(suggestedPort),
    validate: (v) => {
      const n = Number(v);
      if (!Number.isInteger(n) || n < 1024 || n > 65535)
        return "Must be an integer between 1024 and 65535";
    },
  });
  if (p.isCancel(widgetPortStr)) cancel();

  const orgScope = await detectOrgScope(cwd);

  const spec: WidgetSpec = {
    widgetName: widgetName as string,
    widgetTitle: widgetTitle as string,
    widgetPort: Number(widgetPortStr),
    orgScope,
  };

  p.note(
    [
      `Widget:   apps/${spec.widgetName}/`,
      `Title:    ${spec.widgetTitle}`,
      `Port:     ${spec.widgetPort}`,
      `Package:  @${spec.orgScope}/${spec.widgetName}`,
      `Variant:  ${project.variant}`,
    ].join("\n"),
    "Summary"
  );

  const go = await p.confirm({ message: "Create widget?", initialValue: true });
  if (p.isCancel(go) || !go) cancel();

  await renderWidgetFiles(spec, appsDir);
  await registerWidget(spec, cwd, project.variant);

  p.outro(
    [
      `Created apps/${spec.widgetName}/`,
      "",
      "Next steps:",
      `  npm install`,
      `  npm run dev --workspace apps/${spec.widgetName}`,
      "  # restart Terminal to see the widget in the launcher",
    ].join("\n")
  );
}

async function renderWidgetFiles(spec: WidgetSpec, appsDir: string): Promise<void> {
  const destDir = join(appsDir, spec.widgetName);
  const entries = await walk(TEMPLATE_DIR);

  for (const { absolute: src, relative } of entries) {
    const rendered = await ejs.renderFile(src, spec, { async: true });
    const dest = join(destDir, relative.replace(/\.ejs$/, ""));
    await mkdir(dirname(dest), { recursive: true });
    await writeFile(dest, rendered, "utf8");
  }
}

async function registerWidget(spec: WidgetSpec, cwd: string, variant: Variant): Promise<void> {
  if (variant === "standalone") {
    await appendToWidgetsConfig(spec, cwd);
    return;
  }

  // Enterprise — print the curl command. Optionally execute if OT_APPD_TOKEN
  // and OT_APP_DIR_URL are both set.
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
    const ok = await tryRegisterViaApi(url, payload, token);
    if (ok) {
      p.log.success(`Registered with App Directory at ${url}`);
      return;
    }
  }

  p.log.info(
    [
      "Register the widget with your App Directory:",
      "",
      `  curl -X POST ${url} \\`,
      `    -H 'Content-Type: application/json' \\`,
      token ? `    -H 'Authorization: Bearer $OT_APPD_TOKEN' \\` : "",
      `    -d '${JSON.stringify(payload)}'`,
    ]
      .filter(Boolean)
      .join("\n")
  );
}

async function tryRegisterViaApi(url: string, payload: unknown, token: string): Promise<boolean> {
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

async function appendToWidgetsConfig(spec: WidgetSpec, cwd: string): Promise<void> {
  const configPath = join(cwd, "widgets.config.json");
  let parsed: { widgets?: unknown[]; [k: string]: unknown };
  try {
    const raw = await readFile(configPath, "utf8");
    parsed = JSON.parse(raw);
  } catch {
    parsed = { widgets: [] };
  }
  const widgets = Array.isArray(parsed.widgets) ? (parsed.widgets as unknown[]) : [];
  widgets.push({
    appId: spec.widgetName,
    title: spec.widgetTitle,
    url: `http://localhost:${spec.widgetPort}`,
    categories: [],
  });
  parsed.widgets = widgets;
  await writeFile(configPath, JSON.stringify(parsed, null, 2) + "\n", "utf8");
  p.log.success("Updated widgets.config.json");
}

async function suggestPort(appsDir: string): Promise<number> {
  const used = new Set<number>();
  const items = await readdir(appsDir).catch(() => []);
  for (const item of items) {
    const serverPath = join(appsDir, item, "server.js");
    try {
      const text = await readFile(serverPath, "utf8");
      const match = /(?:const|let|var)\s+PORT\s*=\s*(\d{4,5})/.exec(text);
      if (match) used.add(Number(match[1]));
    } catch {
      // no server.js; ignore
    }
  }
  let port = 3010;
  while (used.has(port)) port++;
  return port;
}

async function detectOrgScope(cwd: string): Promise<string> {
  // Read the workspace root package.json and pull the org scope out of any
  // existing @scope/name dependency declaration. Fall back to workspace name.
  try {
    const raw = await readFile(join(cwd, "package.json"), "utf8");
    const pkg = JSON.parse(raw) as { name?: string };
    if (typeof pkg.name === "string" && pkg.name) return pkg.name;
  } catch {
    // fall through
  }
  return "workspace";
}

async function dirExists(path: string): Promise<boolean> {
  try {
    const info = await stat(path);
    return info.isDirectory();
  } catch {
    return false;
  }
}

function toTitleCase(s: string): string {
  return s
    .split("-")
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join(" ");
}

function cancel(): never {
  p.cancel("Cancelled.");
  process.exit(0);
}
