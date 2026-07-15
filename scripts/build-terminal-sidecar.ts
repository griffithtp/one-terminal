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
 *
 * Target selection: honors `TAURI_ENV_TARGET_TRIPLE` (set by `tauri build
 * --target <triple>`, e.g. the release workflow's cross-compilation matrix)
 * so the sidecar is built for the triple actually being packaged, not just
 * the host default. Falls back to the host triple (`rustc -vV`) for local
 * `npm run build:desktop-agent` runs with no explicit `--target`.
 *
 * NOTE: `universal-apple-darwin` is NOT handled here — that target needs
 * both `aarch64-apple-darwin` and `x86_64-apple-darwin` sidecar binaries
 * present before `tauri build` lipos them together, which this script
 * doesn't attempt (unverified — see docs/plans/07-deployment-and-hosting.md
 * Issue 07-A). Release workflow currently targets `aarch64-apple-darwin`
 * only on macOS.
 */
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..");

function hostTriple(): string {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf-8" });
  const match = out.match(/^host:\s*(\S+)$/m);
  if (!match) {
    throw new Error(
      "build-terminal-sidecar: could not determine host target triple from `rustc -vV`"
    );
  }
  return match[1];
}

const explicitTarget = process.env.TAURI_ENV_TARGET_TRIPLE;
const host = hostTriple();
const triple = explicitTarget ?? host;
const crossCompiling = triple !== host;
const isWindows = triple.includes("windows");
const exeSuffix = isWindows ? ".exe" : "";

console.log(
  `[build-terminal-sidecar] building one-terminal (release, ${triple}${crossCompiling ? `, cross-compiled from ${host}` : ""})…`
);

const cargoArgs = ["build", "--release", "-p", "one-terminal"];
if (crossCompiling) {
  cargoArgs.push("--target", triple);
}
execFileSync("cargo", cargoArgs, { cwd: repoRoot, stdio: "inherit" });

const src = crossCompiling
  ? join(repoRoot, "target", triple, "release", `one-terminal${exeSuffix}`)
  : join(repoRoot, "target", "release", `one-terminal${exeSuffix}`);
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
