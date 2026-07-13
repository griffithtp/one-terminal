#!/usr/bin/env node
/**
 * Builds `one-terminal` in release mode and copies it into
 * `apps/desktop-agent/src-tauri/binaries/` under Tauri's `externalBin`
 * naming convention (`<name>-<target-triple>[.exe]`), so `desktop-agent`'s
 * packaged bundle can auto-launch it (see `spawn_wm_instance()` /
 * `locate_wm_binary()` in `apps/desktop-agent/src-tauri/src/lib.rs`, and the
 * `bundle.externalBin` entry in `apps/desktop-agent/src-tauri/tauri.conf.json`).
 *
 * Runs as part of `desktop-agent`'s `build` script (i.e. only on the
 * `tauri build` / release path) — `tauri dev` doesn't need this, since
 * `cargo`'s shared `target/debug/` directory already puts `one-terminal`
 * right next to `desktop-agent` for free.
 */
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..");

function targetTriple(): string {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf-8" });
  const match = out.match(/^host:\s*(\S+)$/m);
  if (!match) {
    throw new Error(
      "build-terminal-sidecar: could not determine host target triple from `rustc -vV`"
    );
  }
  return match[1];
}

const triple = targetTriple();
const isWindows = process.platform === "win32";
const exeSuffix = isWindows ? ".exe" : "";

console.log(`[build-terminal-sidecar] building one-terminal (release, ${triple})…`);
execFileSync("cargo", ["build", "--release", "-p", "one-terminal"], {
  cwd: repoRoot,
  stdio: "inherit",
});

const src = join(repoRoot, "target", "release", `one-terminal${exeSuffix}`);
if (!existsSync(src)) {
  throw new Error(`build-terminal-sidecar: expected binary not found at ${src}`);
}

const binariesDir = join(repoRoot, "apps", "desktop-agent", "src-tauri", "binaries");
mkdirSync(binariesDir, { recursive: true });

const dest = join(binariesDir, `one-terminal-${triple}${exeSuffix}`);
copyFileSync(src, dest);
if (!isWindows) {
  chmodSync(dest, 0o755);
}

console.log(`[build-terminal-sidecar] wrote ${dest}`);
