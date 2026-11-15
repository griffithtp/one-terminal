#!/usr/bin/env node
// PostToolUse (Edit|Write|MultiEdit): format the file Claude just changed with the
// repo's own tools — Prettier (respecting .prettierignore) or rustfmt — so formatting
// never reaches review. Never blocks: a formatter failure is only reported.

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");

// Hook stdout must be a single JSON object, so collect everything and write once.
const output = {};

function finish() {
  if (Object.keys(output).length) process.stdout.write(JSON.stringify(output));
  process.exit(0);
}

function report(message) {
  output.systemMessage = `format hook: ${message}`;
  finish();
}

let input;
try {
  input = JSON.parse(readFileSync(0, "utf8"));
} catch {
  finish();
}
const file = input?.tool_input?.file_path ?? input?.tool_response?.filePath;
if (!file) finish();
const rel = relative(ROOT, file).split(sep).join("/");
if (rel.startsWith("../")) finish();

if (basename(file) === "AGENTS.md") {
  output.hookSpecificOutput = {
    hookEventName: "PostToolUse",
    additionalContext:
      "You edited an AGENTS.md source. Run `npm run sync:ai` to regenerate its CLAUDE.md / Copilot adapters before finishing.",
  };
}

if (file.endsWith(".rs")) {
  const result = spawnSync("rustfmt", ["--edition", "2021", file], { encoding: "utf8" });
  if (result.error) report(`rustfmt not found (${result.error.message})`);
  if (result.status !== 0) report(`rustfmt failed on ${rel}: ${result.stderr.trim()}`);
  finish();
}

let prettier;
try {
  prettier = await import("prettier");
} catch {
  finish(); // node_modules not installed yet
}
try {
  const info = await prettier.getFileInfo(file, { ignorePath: join(ROOT, ".prettierignore") });
  if (!info.ignored && info.inferredParser) {
    const source = readFileSync(file, "utf8");
    const options = (await prettier.resolveConfig(file)) ?? {};
    const formatted = await prettier.format(source, { ...options, filepath: file });
    if (formatted !== source) writeFileSync(file, formatted, "utf8");
  }
} catch (e) {
  report(`prettier failed on ${rel}: ${e instanceof Error ? e.message.split("\n")[0] : e}`);
}
finish();
