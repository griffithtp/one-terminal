#!/usr/bin/env tsx
/**
 * Checks that the docs match the code (docs/conventions/docs.md):
 *
 *   env vars      every `OT_*` variable read in source is in an AGENTS.md env table, and
 *                 every documented `OT_*` variable is read somewhere
 *   layout        every app/package directory is covered by the AGENTS.md workspace layout
 *   agents        every `.ai/agents/*.md` and `.ai/skills/*` is listed in the index tables
 *   plan refs     no committed file names a plan or links into `.work/`
 *   links         relative links and `#anchors` in Markdown and code doc comments resolve,
 *                 with exact case, to tracked files inside the repo
 *
 * Files come from `git ls-files` (tracked plus untracked-but-not-ignored), so gitignored
 * paths such as `.work/` and build output are never scanned. Output is identical on
 * Windows and macOS: paths are repo-relative POSIX, text is compared on LF.
 *
 * Usage:
 *   npx tsx scripts/check-docs.ts      # exit 1 and print `path:line: message` per problem
 */

import { execFileSync } from "node:child_process";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { join, posix } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(import.meta.url), "../..");

export interface Finding {
  file: string;
  line: number;
  message: string;
}

// Generated or published-history paths: checked through their sources, or not at all.
const GENERATED_PREFIXES = ["packages/create-one-terminal/templates/", ".claude/"];
const GENERATED_FILES = new Set(["packages/fdc3-plugin/fdc3-plugin.js"]);

const SOURCE_EXTENSIONS = /\.(rs|ts|tsx|js|mjs|cjs)$/;
const BINARY_EXTENSIONS =
  /\.(png|jpe?g|gif|ico|icns|webp|bmp|svg|woff2?|ttf|otf|eot|pdf|zip|gz|tgz|wasm|exe|dll|dylib|so)$/i;
const LOCKFILES = new Set(["package-lock.json", "Cargo.lock"]);

/** 1-based line number of a character offset. */
function lineAt(text: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset; i++) if (text.charCodeAt(i) === 10) line++;
  return line;
}

function isGenerated(path: string): boolean {
  return (
    GENERATED_FILES.has(path) ||
    GENERATED_PREFIXES.some((p) => path.startsWith(p)) ||
    posix.basename(path) === "CLAUDE.md"
  );
}

// ── Env vars ─────────────────────────────────────────────────────────────────

export interface EnvRead {
  name: string;
  file: string;
  line: number;
}

/** Test code doesn't define the product's env surface. */
function isTestFile(path: string): boolean {
  return /(^|\/)tests?\//.test(path) || /\.test\.[cm]?[jt]sx?$/.test(path);
}

/** `OT_*` env var read sites in one source file. */
export function findEnvReads(file: string, text: string): EnvRead[] {
  if (isTestFile(file)) return [];
  const reads: EnvRead[] = [];
  let body = text;

  if (file.endsWith(".rs")) {
    // Drop the `#[cfg(test)]` module that conventionally closes a Rust file.
    const testModule = body.search(/^#\[cfg\(test\)\]/m);
    if (testModule >= 0) body = body.slice(0, testModule);

    for (const m of body.matchAll(/\b(?:var|var_os)\(\s*"(OT_[A-Z0-9_]+)"\s*\)/g)) {
      reads.push({ name: m[1], file, line: lineAt(body, m.index) });
    }
    const consts = new Map<string, string>();
    for (const m of body.matchAll(
      /\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"(OT_[A-Z0-9_]+)"/g
    )) {
      consts.set(m[1], m[2]);
    }
    for (const m of body.matchAll(/\b(?:var|var_os)\(\s*([A-Z][A-Z0-9_]*)\s*\)/g)) {
      const name = consts.get(m[1]);
      if (name) reads.push({ name, file, line: lineAt(body, m.index) });
    }
  } else {
    for (const m of body.matchAll(
      /process\.env(?:\.(OT_[A-Z0-9_]+)\b|\[\s*["'](OT_[A-Z0-9_]+)["']\s*\])/g
    )) {
      reads.push({ name: m[1] ?? m[2], file, line: lineAt(body, m.index) });
    }
  }
  return reads;
}

/** Text of a `## Heading` section (up to the next `## `), with the line it starts on. */
function section(markdown: string, heading: string): { text: string; startLine: number } | null {
  const start = markdown.search(new RegExp(`^## ${heading}\\s*$`, "m"));
  if (start < 0) return null;
  const rest = markdown.slice(start + 3);
  const next = rest.search(/^## /m);
  return {
    text: markdown.slice(start, next < 0 ? undefined : start + 3 + next),
    startLine: lineAt(markdown, start),
  };
}

/** Variables in the first column of every table under AGENTS.md → Environment variables. */
export function parseEnvTables(agentsMd: string): Map<string, number> {
  const documented = new Map<string, number>();
  const env = section(agentsMd, "Environment variables");
  if (!env) return documented;
  env.text.split("\n").forEach((row, i) => {
    const m = /^\|\s*`([A-Z][A-Z0-9_]*)`\s*\|/.exec(row);
    if (m) documented.set(m[1], env.startLine + i);
  });
  return documented;
}

export function checkEnvVars(reads: EnvRead[], documented: Map<string, number>): Finding[] {
  const findings: Finding[] = [];
  const read = new Set<string>();
  for (const r of reads) {
    read.add(r.name);
    if (!documented.has(r.name)) {
      findings.push({
        file: r.file,
        line: r.line,
        message: `${r.name} is read here but not documented in AGENTS.md → Environment variables`,
      });
    }
  }
  for (const [name, line] of documented) {
    // Non-OT_ rows (PORT, CORS_ORIGIN) are third-party conventions; only OT_ is ours to track.
    if (name.startsWith("OT_") && !read.has(name)) {
      findings.push({
        file: "AGENTS.md",
        line,
        message: `${name} is documented but no source reads it`,
      });
    }
  }
  return findings;
}

// ── Workspace layout ─────────────────────────────────────────────────────────

/** Paths (no trailing slash) listed in the AGENTS.md → Workspace layout code block. */
export function parseLayout(agentsMd: string): Map<string, number> {
  const listed = new Map<string, number>();
  const layout = section(agentsMd, "Workspace layout");
  if (!layout) return listed;
  layout.text.split("\n").forEach((row, i) => {
    const m = /^((?:apps|packages)\/\S+)/.exec(row);
    if (m) listed.set(m[1].replace(/\/$/, ""), layout.startLine + i);
  });
  return listed;
}

export function checkLayout(
  listed: Map<string, number>,
  dirs: string[],
  exists: (path: string) => boolean,
  layoutLine = 1
): Finding[] {
  const findings: Finding[] = [];
  for (const dir of dirs) {
    const covered = [...listed.keys()].some((p) => p === dir || p.startsWith(`${dir}/`));
    if (!covered) {
      findings.push({
        file: "AGENTS.md",
        line: layoutLine,
        message: `${dir}/ is not covered by the Workspace layout block`,
      });
    }
  }
  for (const [path, line] of listed) {
    if (!exists(path)) {
      findings.push({
        file: "AGENTS.md",
        line,
        message: `layout lists ${path}/, which doesn't exist`,
      });
    }
  }
  return findings;
}

// ── Agents and skills index ──────────────────────────────────────────────────

export function checkAgentIndex(
  agents: string[],
  skills: string[],
  agentsMd: string,
  aiReadme: string
): Finding[] {
  const findings: Finding[] = [];
  for (const name of agents) {
    const source = `.ai/agents/${name}.md`;
    if (!agentsMd.includes(`(${source})`)) {
      findings.push({
        file: source,
        line: 1,
        message: `${name} is not in the AGENTS.md agents table`,
      });
    }
    if (!aiReadme.includes(`(agents/${name}.md)`)) {
      findings.push({
        file: source,
        line: 1,
        message: `${name} is not in the .ai/README.md agents table`,
      });
    }
  }
  for (const name of skills) {
    const source = `.ai/skills/${name}/SKILL.md`;
    if (!agentsMd.includes(`(${source})`)) {
      findings.push({
        file: source,
        line: 1,
        message: `${name} is not in the AGENTS.md workflow table`,
      });
    }
  }
  return findings;
}

// ── Plan references ──────────────────────────────────────────────────────────

// The checker and its tests contain every pattern on purpose, as code and fixtures.
const CHECKER_FILES = new Set(["scripts/check-docs.ts", "scripts/check-docs.test.ts"]);

// These name or match the patterns on purpose: the checker, its fixtures, and history.
const PLAN_REF_EXEMPT = new Set([
  ...CHECKER_FILES,
  "packages/create-one-terminal/versions.json",
  ".gitignore",
]);

// These tell people and agents where the planning folder is, so they may name `.work/`.
const WORK_FOLDER_ALLOWED = new Set([
  "AGENTS.md",
  "docs/conventions/docs.md",
  ".ai/agents/requirements-analyst.md",
  ".ai/agents/architect.md",
  ".ai/agents/performance-analyst.md",
  ".ai/agents/docs-steward.md",
  ".ai/skills/feature/SKILL.md",
]);

const PLAN_PATTERNS: Array<{ re: RegExp; what: string; workFolder?: true }> = [
  { re: /\.work\//g, what: "the uncommitted .work/ folder", workFolder: true },
  { re: /docs\/plans\//g, what: "docs/plans/ (plans live in .work/, never committed)" },
  { re: /docs\/dashboards\//g, what: "docs/dashboards/ (moved to .work/)" },
  { re: /Terminal Menu\.md/g, what: "Terminal Menu.md (moved to .work/)" },
  { re: /\b(?:Plan|Epic) \d{2}\b/g, what: "a plan number" },
];

export function findPlanReferences(file: string, text: string): Finding[] {
  if (PLAN_REF_EXEMPT.has(file)) return [];
  const findings: Finding[] = [];
  for (const { re, what, workFolder } of PLAN_PATTERNS) {
    if (workFolder && WORK_FOLDER_ALLOWED.has(file)) continue;
    for (const m of text.matchAll(re)) {
      findings.push({
        file,
        line: lineAt(text, m.index),
        message: `refers to ${what}: describe the behaviour or decision instead`,
      });
    }
  }
  return findings;
}

// ── Links ────────────────────────────────────────────────────────────────────

export interface Link {
  target: string;
  line: number;
}

/**
 * Blank out fenced code blocks and (unless `inline` is false) inline code, keeping offsets
 * and line numbers. Headings keep their inline code: GitHub slugs include its text.
 */
function stripCode(markdown: string, inline = true): string {
  const blank = (s: string) => " ".repeat(s.length);
  let fence: string | null = null;
  return markdown
    .split("\n")
    .map((line) => {
      const marker = /^ {0,3}(`{3,}|~{3,})/.exec(line)?.[1];
      if (fence === null && marker) {
        fence = marker;
        return blank(line);
      }
      if (fence !== null) {
        if (marker && marker[0] === fence[0] && marker.length >= fence.length) fence = null;
        return blank(line);
      }
      return inline ? line.replace(/(`+)(?!`).*?(?<!`)\1(?!`)/g, blank) : line;
    })
    .join("\n");
}

/** `[text](target)`, `![alt](src)` and HTML `href` / `src` links outside code. */
export function extractMarkdownLinks(markdown: string): Link[] {
  const text = stripCode(markdown);
  const links: Link[] = [];
  for (const m of text.matchAll(/\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/g)) {
    links.push({ target: m[1], line: lineAt(text, m.index) });
  }
  for (const m of text.matchAll(/<(?:a|img)\b[^>]*?\s(?:href|src)="([^"]+)"/g)) {
    links.push({ target: m[1], line: lineAt(text, m.index) });
  }
  return links;
}

/**
 * `[text](target)` links in doc comments: Rust `//!` / `///` lines and JS/TS `/** … *\/`
 * blocks. Only path-like targets count, so Rust intra-doc links (`[x](crate::y)`) and
 * links inside string literals are ignored.
 */
export function extractCommentLinks(file: string, source: string): Link[] {
  const links: Link[] = [];
  const add = (chunk: string, offset: number) => {
    for (const m of chunk.matchAll(/\[[^\]\n]*\]\(([^)\s]+)\)/g)) {
      const target = m[1];
      if (target.includes("::") || !/[/.]/.test(target)) continue;
      links.push({ target, line: lineAt(source, offset + m.index) });
    }
  };
  if (file.endsWith(".rs")) {
    for (const m of source.matchAll(/^[ \t]*\/\/[!/][^\n]*/gm)) add(m[0], m.index);
  } else {
    for (const m of source.matchAll(/\/\*\*[\s\S]*?\*\//g)) add(m[0], m.index);
  }
  return links;
}

/** GitHub's heading anchor for a heading's text. */
export function slug(heading: string): string {
  let text = heading;
  let previous: string;
  do {
    previous = text;
    text = text
      .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/<[^>]+>/g, "");
  } while (text !== previous);

  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\p{M} _-]/gu, "")
    .replace(/ /g, "-");
}

/** Every anchor a Markdown file defines: heading slugs (with -1, -2 … for repeats) and HTML ids. */
export function anchors(markdown: string): Set<string> {
  const text = stripCode(markdown, false);
  const seen = new Map<string, number>();
  const found = new Set<string>();
  for (const m of text.matchAll(/^ {0,3}#{1,6}[ \t]+(.+?)[ \t]*#*[ \t]*$/gm)) {
    const base = slug(m[1]);
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    found.add(n === 0 ? base : `${base}-${n}`);
  }
  for (const m of markdown.matchAll(/<a\s[^>]*(?:id|name)="([^"]+)"/g)) found.add(m[1]);
  return found;
}

export interface Repo {
  /** Tracked files, repo-relative POSIX, exact case. */
  files: Set<string>;
  /** Every directory that contains a tracked file. */
  dirs: Set<string>;
  read: (path: string) => string;
}

export function checkLink(from: string, link: Link, repo: Repo): Finding | null {
  const fail = (message: string): Finding => ({ file: from, line: link.line, message });
  const target = link.target;
  if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith("//")) return null; // external

  const hash = target.indexOf("#");
  const rawPath = hash < 0 ? target : target.slice(0, hash);
  const fragment = hash < 0 ? "" : target.slice(hash + 1);

  let resolved = from;
  if (rawPath !== "") {
    let decoded: string;
    try {
      decoded = decodeURIComponent(rawPath);
    } catch {
      return fail(`malformed link target ${target}`);
    }
    resolved = posix.normalize(posix.join(posix.dirname(from), decoded)).replace(/\/$/, "");
    if (resolved === ".." || resolved.startsWith("../")) {
      return fail(`${target} points outside the repository; use an absolute URL`);
    }
    if (resolved === ".work" || resolved.startsWith(".work/")) {
      return fail(`${target} points into .work/, which is never committed`);
    }
    if (!repo.files.has(resolved) && !repo.dirs.has(resolved) && resolved !== ".") {
      const lower = resolved.toLowerCase();
      const caseMatch = [...repo.files, ...repo.dirs].find((p) => p.toLowerCase() === lower);
      return fail(
        caseMatch
          ? `${target}: case mismatch, the file is ${caseMatch} (breaks on Linux and GitHub)`
          : `${target}: no such file`
      );
    }
  }

  if (fragment === "") return null;
  if (!resolved.endsWith(".md")) {
    return /^L\d+(-L\d+)?$/.test(fragment) || repo.dirs.has(resolved)
      ? null
      : fail(`${target}: #${fragment} only works on Markdown files (or #L<n> line links)`);
  }
  let anchor: string;
  try {
    anchor = decodeURIComponent(fragment).toLowerCase();
  } catch {
    return fail(`malformed anchor #${fragment}`);
  }
  return anchors(repo.read(resolved)).has(anchor)
    ? null
    : fail(`${target}: no heading #${fragment} in ${resolved}`);
}

// ── Main ─────────────────────────────────────────────────────────────────────

function listFiles(): string[] {
  const out = execFileSync(
    "git",
    ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }
  );
  // Skip files deleted in the work tree (still in the index) and symlinks, which could
  // point outside the repo.
  return [...new Set(out.split("\0").filter(Boolean))].filter((f) => {
    try {
      return !lstatSync(join(ROOT, f)).isSymbolicLink();
    } catch {
      return false;
    }
  });
}

/** LF line endings and no BOM: Git for Windows may check out CRLF, and some editors add a BOM. */
export function normalizeText(text: string): string {
  return text.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
}

function readText(path: string): string {
  return normalizeText(readFileSync(join(ROOT, path), "utf8"));
}

export function runChecks(): Finding[] {
  const files = listFiles();
  const fileSet = new Set(files);
  const dirSet = new Set<string>();
  for (const f of files) {
    for (let d = posix.dirname(f); d !== "."; d = posix.dirname(d)) dirSet.add(d);
  }
  const cache = new Map<string, string>();
  const read = (p: string) => {
    let text = cache.get(p);
    if (text === undefined) cache.set(p, (text = readText(p)));
    return text;
  };
  const repo: Repo = { files: fileSet, dirs: dirSet, read };
  const agentsMd = read("AGENTS.md");
  const findings: Finding[] = [];

  const sources = files.filter((f) => SOURCE_EXTENSIONS.test(f) && !isGenerated(f));

  // Env vars
  const reads = sources.flatMap((f) => findEnvReads(f, read(f)));
  findings.push(...checkEnvVars(reads, parseEnvTables(agentsMd)));

  // Workspace layout
  const workspaceDirs = [...dirSet]
    .filter((d) => /^(apps|packages)\/[^/]+$/.test(d))
    .filter((d) => fileSet.has(`${d}/package.json`) || fileSet.has(`${d}/Cargo.toml`))
    .sort();
  findings.push(
    ...checkLayout(
      parseLayout(agentsMd),
      workspaceDirs,
      (p) => dirSet.has(p),
      section(agentsMd, "Workspace layout")?.startLine
    )
  );

  // Agents and skills index
  const agents = files
    .map((f) => /^\.ai\/agents\/([^/]+)\.md$/.exec(f)?.[1])
    .filter((n): n is string => !!n);
  const skills = files
    .map((f) => /^\.ai\/skills\/([^/]+)\/SKILL\.md$/.exec(f)?.[1])
    .filter((n): n is string => !!n);
  findings.push(...checkAgentIndex(agents, skills, agentsMd, read(".ai/README.md")));

  // Plan references
  for (const f of files) {
    if (isGenerated(f) || BINARY_EXTENSIONS.test(f) || LOCKFILES.has(posix.basename(f))) continue;
    const text = read(f);
    if (text.includes("\0")) continue;
    findings.push(...findPlanReferences(f, text));
  }

  // Links
  for (const f of files) {
    if (isGenerated(f)) continue;
    const links = f.endsWith(".md")
      ? extractMarkdownLinks(read(f))
      : SOURCE_EXTENSIONS.test(f) && !CHECKER_FILES.has(f)
        ? extractCommentLinks(f, read(f))
        : [];
    for (const link of links) {
      const finding = checkLink(f, link, repo);
      if (finding) findings.push(finding);
    }
  }

  // Plain code-unit order, so output is identical whatever the machine's locale.
  return findings.sort((a, b) => (a.file === b.file ? a.line - b.line : a.file < b.file ? -1 : 1));
}

/** True when this file is the entry point. Compares real paths, so a symlinked, junction or
 * `subst` checkout still runs the checks instead of silently exiting 0. */
function isEntryPoint(): boolean {
  const entry = process.argv[1];
  if (!entry) return false;
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
}

if (isEntryPoint()) {
  const findings = runChecks();
  for (const f of findings) console.error(`${f.file}:${f.line}: ${f.message}`);
  if (findings.length > 0) {
    console.error(`\ncheck:docs: ${findings.length} problem(s). See docs/conventions/docs.md.`);
    process.exit(1);
  }
  console.log("✓ Docs match the code.");
}
