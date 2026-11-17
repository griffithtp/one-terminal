// Tests for scripts/ci-changes.mjs against real temporary git repos. Run: npm run test:scripts
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, describe, it } from "node:test";

import { isMarkdownOnly, needsBuild } from "./ci-changes.mjs";

const repos: string[] = [];
after(() => {
  for (const dir of repos) rmSync(dir, { recursive: true, force: true });
});

/** A repo with one base commit; returns its dir, the base sha, and a git helper. */
function repo() {
  const dir = mkdtempSync(join(tmpdir(), "ci-changes-"));
  repos.push(dir);
  const git = (...args: string[]) =>
    execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
  git("init", "-q");
  git("config", "user.email", "test@example.com");
  git("config", "user.name", "test");
  git("config", "commit.gpgsign", "false");
  writeFileSync(join(dir, "lib.rs"), "fn main() {}\n");
  writeFileSync(join(dir, "README.md"), "# Readme\n");
  git("add", ".");
  git("commit", "-q", "-m", "base");
  return { dir, base: git("rev-parse", "HEAD"), git };
}

const commit = (git: (...a: string[]) => string) => {
  git("add", "-A");
  git("commit", "-q", "-m", "change");
};

describe("isMarkdownOnly", () => {
  it("is false for an empty change set and for any non-.md path", () => {
    assert.equal(isMarkdownOnly([]), false);
    assert.equal(isMarkdownOnly(["a.md", "b.rs"]), false);
    assert.equal(isMarkdownOnly(["README.MD"]), false);
    assert.equal(isMarkdownOnly(["a.md", "docs/b.md"]), true);
  });
});

describe("needsBuild", () => {
  it("skips a pull request that only edits Markdown", () => {
    const { dir, base, git } = repo();
    writeFileSync(join(dir, "README.md"), "# Changed\n");
    commit(git);
    assert.equal(needsBuild({ event: "pull_request", base, cwd: dir }), false);
  });

  it("builds when a code file is renamed to .md", () => {
    const { dir, base, git } = repo();
    git("mv", "lib.rs", "lib.md");
    commit(git);
    assert.equal(needsBuild({ event: "pull_request", base, cwd: dir }), true);
  });

  it("builds when a code file is deleted or changed", () => {
    const { dir, base, git } = repo();
    git("rm", "-q", "lib.rs");
    commit(git);
    assert.equal(needsBuild({ event: "pull_request", base, cwd: dir }), true);
  });

  it("builds on push, even for a Markdown-only change", () => {
    const { dir, base, git } = repo();
    writeFileSync(join(dir, "README.md"), "# Changed\n");
    commit(git);
    assert.equal(needsBuild({ event: "push", base, cwd: dir }), true);
  });

  it("builds for an empty diff, a missing base or an unknown base", () => {
    const { dir, base } = repo();
    assert.equal(needsBuild({ event: "pull_request", base, cwd: dir }), true);
    assert.equal(needsBuild({ event: "pull_request", base: "", cwd: dir }), true);
    assert.equal(needsBuild({ event: "pull_request", base: "0".repeat(40), cwd: dir }), true);
  });
});
