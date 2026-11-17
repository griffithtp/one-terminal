#!/usr/bin/env node
/**
 * CI change detection: does this pull request touch anything besides Markdown?
 *
 * Writes `code=true|false` to $GITHUB_OUTPUT. Build and test steps run only when
 * `code=true`; a docs-only PR (`code=false`) is checked by the separate `docs` job.
 *
 * Fails safe: anything other than a pull request whose diff against its base is non-empty
 * and entirely `*.md` gives `code=true`. In particular:
 *   - pushes always build (skipping on push could hide an earlier failure on main);
 *   - the diff is against the PR base, not the previous push, so a docs commit can't turn
 *     a failing PR green;
 *   - `--no-renames` lists a renamed file's old path, so renaming `x.rs` to `x.md` counts
 *     as a code change.
 *
 * Plain Node (no dependencies) so it runs on every runner before `npm ci`.
 * Usage (CI): EVENT=<event_name> BASE_SHA=<pr base sha> node scripts/ci-changes.mjs
 */

import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

/** True when `files` is non-empty and every path ends in `.md` (case-sensitive). */
export function isMarkdownOnly(files) {
  return files.length > 0 && files.every((f) => f.endsWith(".md"));
}

/** Paths changed between `base` and HEAD, renames split into delete + add. */
export function changedFiles(base, cwd = process.cwd()) {
  const out = execFileSync("git", ["diff", "--no-renames", "--name-only", "-z", base, "HEAD"], {
    cwd,
    encoding: "utf8",
  });
  return out.split("\0").filter(Boolean);
}

/** Whether build and test steps must run for this event. */
export function needsBuild({ event, base, cwd }) {
  if (event !== "pull_request" || !base) return true;
  try {
    execFileSync("git", ["cat-file", "-e", `${base}^{commit}`], { cwd, stdio: "ignore" });
    return !isMarkdownOnly(changedFiles(base, cwd));
  } catch {
    return true; // base not fetched or unknown: build
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const code = needsBuild({ event: process.env.EVENT, base: process.env.BASE_SHA });
  console.log(
    code
      ? "Code changed: running build and test steps."
      : "Only Markdown changed: skipping build and test steps."
  );
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `code=${code}\n`);
}
