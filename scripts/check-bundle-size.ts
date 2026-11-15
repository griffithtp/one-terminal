#!/usr/bin/env tsx
/**
 * Measures what each web UI loads at first paint and checks it against budgets.
 *
 * For every HTML entry in a built `dist/`, "initial JS" is the entry `<script type=module>`
 * plus every `<link rel=modulepreload>` chunk; "initial CSS" is every stylesheet link.
 * Sizes are gzipped at level 9 (within ~1 KB of the sizes Vite prints).
 * Budgets: scripts/bundle-budgets.json.
 *
 * Usage (build the UIs first — see scripts/bundle-budgets.json for the commands):
 *   npx tsx scripts/check-bundle-size.ts                      # table; exit 1 if over budget
 *   npx tsx scripts/check-bundle-size.ts --json out.json      # also write measurements
 *   npx tsx scripts/check-bundle-size.ts --compare base.json  # show change vs a baseline
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const ROOT = join(fileURLToPath(import.meta.url), "../..");

interface EntryBudget {
  /** Max gzipped KB of the entry script alone (AGENTS.md: < 60 KB). */
  entryKb: number;
  /** Max gzipped KB of all JS loaded at first paint (entry + preloaded chunks). */
  initialJsKb?: number;
  initialCssKb?: number;
}
interface AppBudget {
  dist: string;
  build: string;
  entries: Record<string, EntryBudget>;
}
interface Budgets {
  apps: Record<string, AppBudget>;
}

interface Measurement {
  entryKb: number;
  initialJsKb: number;
  initialCssKb: number;
  files: Record<string, number>;
}
type Results = Record<string, Record<string, Measurement>>;

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

const kb = (bytes: number) => Math.round((bytes / 1024) * 100) / 100;
const gzKb = (path: string) => kb(gzipSync(readFileSync(path), { level: 9 }).length);

/** Asset URLs referenced by an HTML entry, split by how they load. */
function parseHtml(html: string) {
  const attr = (tag: string, name: string) =>
    new RegExp(`\\b${name}\\s*=\\s*"([^"]+)"`).exec(tag)?.[1];
  const tags = html.match(/<(script|link)\b[^>]*>/gi) ?? [];
  const entry: string[] = [];
  const preload: string[] = [];
  const css: string[] = [];
  for (const tag of tags) {
    const tagLower = tag.toLowerCase();
    if (tagLower.startsWith("<script") && /type\s*=\s*"module"/.test(tagLower)) {
      const src = attr(tag, "src");
      if (src) entry.push(src);
    } else if (tagLower.startsWith("<link")) {
      const rel = attr(tag, "rel");
      const href = attr(tag, "href");
      if (!href) continue;
      if (rel === "modulepreload") preload.push(href);
      else if (rel === "stylesheet") css.push(href);
    }
  }
  return { entry, preload, css };
}

const budgets = JSON.parse(
  readFileSync(join(ROOT, "scripts/bundle-budgets.json"), "utf8")
) as Budgets;
const results: Results = {};
const failures: string[] = [];
const rows: string[][] = [];

for (const [app, cfg] of Object.entries(budgets.apps)) {
  const dist = join(ROOT, cfg.dist);
  results[app] = {};
  for (const [html, budget] of Object.entries(cfg.entries)) {
    const htmlPath = join(dist, html);
    if (!existsSync(htmlPath)) {
      failures.push(`${app}/${html}: not built — run \`${cfg.build}\``);
      continue;
    }
    const { entry, preload, css } = parseHtml(readFileSync(htmlPath, "utf8"));
    const local = (url: string) => join(dist, url.replace(/^\.?\//, ""));
    const files: Record<string, number> = {};
    for (const url of [...entry, ...preload, ...css]) files[url] = gzKb(local(url));
    const sum = (urls: string[]) => kb(urls.reduce((t, u) => t + files[u] * 1024, 0));

    const m: Measurement = {
      entryKb: sum(entry),
      initialJsKb: sum([...entry, ...preload]),
      initialCssKb: sum(css),
      files,
    };
    results[app][html] = m;

    const check = (label: string, value: number, limit?: number) => {
      if (limit !== undefined && value > limit) {
        failures.push(`${app}/${html}: ${label} ${value} KB > budget ${limit} KB`);
      }
    };
    check("entry", m.entryKb, budget.entryKb);
    check("initial JS", m.initialJsKb, budget.initialJsKb);
    check("initial CSS", m.initialCssKb, budget.initialCssKb);
    rows.push([
      `${app}/${html}`,
      `${m.entryKb} / ${budget.entryKb}`,
      `${m.initialJsKb} / ${budget.initialJsKb ?? "–"}`,
      `${m.initialCssKb} / ${budget.initialCssKb ?? "–"}`,
    ]);
  }
}

// ── Output ────────────────────────────────────────────────────────────────────

const header = ["entry (gzip KB, used / budget)", "entry JS", "initial JS", "initial CSS"];
const widths = header.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i].length)));
const line = (cells: string[]) => cells.map((c, i) => c.padEnd(widths[i])).join("  ");
console.log(line(header));
console.log(widths.map((w) => "-".repeat(w)).join("  "));
for (const r of rows) console.log(line(r));

const baselinePath = arg("--compare");
if (baselinePath) {
  const base = JSON.parse(readFileSync(baselinePath, "utf8")) as Results;
  console.log(`\nChange vs ${baselinePath} (gzip KB):`);
  for (const [app, entries] of Object.entries(results)) {
    for (const [html, m] of Object.entries(entries)) {
      const b = base[app]?.[html];
      if (!b) {
        console.log(`  ${app}/${html}: new entry`);
        continue;
      }
      const d = (x: number, y: number) => `${x - y >= 0 ? "+" : ""}${kb((x - y) * 1024)}`;
      console.log(
        `  ${app}/${html}: entry ${d(m.entryKb, b.entryKb)}, initial JS ${d(m.initialJsKb, b.initialJsKb)}, CSS ${d(m.initialCssKb, b.initialCssKb)}`
      );
    }
  }
}

const jsonPath = arg("--json");
if (jsonPath) writeFileSync(jsonPath, JSON.stringify(results, null, 2) + "\n");

if (failures.length > 0) {
  console.error(`\nBundle budget check failed:\n${failures.map((f) => `  • ${f}`).join("\n")}`);
  console.error(
    "\nShrink the bundle (lazy-load, split vendor code, drop a dependency) or, if the growth" +
      " is justified, raise the budget in scripts/bundle-budgets.json in the same PR and say why."
  );
  process.exit(1);
}
console.log("\n✓ All UIs within budget.");
