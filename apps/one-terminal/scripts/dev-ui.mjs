#!/usr/bin/env node
/**
 * `tauri dev`'s beforeDevCommand for the Terminal: serve the Terminal UI
 * (Vite, http://localhost:1422) unless something already serves it.
 *
 * A debug `one-terminal` binary loads its UI from that server. Both
 * `npm run dev:terminal` and the Desktop Agent's dev loop (which starts
 * Terminals) need it, and `npm run dev:all` runs both — with `strictPort`, the
 * second Vite would fail and take its `tauri dev` with it. So whoever comes
 * second reuses the running server — and takes over if that one stops.
 *
 * Also imported by `apps/desktop-agent/scripts/dev.mjs`.
 */
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const TERMINAL_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
export const TERMINAL_UI_PORT = 1422;

/** True when something answers HTTP on `localhost:<port>`. */
export async function isServed(port) {
  try {
    await fetch(`http://localhost:${port}/`, { signal: AbortSignal.timeout(1_000) });
    return true;
  } catch {
    return false;
  }
}

/** How often a process that reuses another's server checks it is still there. */
const RECHECK_MS = 2_000;

function startVite(dir) {
  // Vite's `exports` don't include its bin script; find it from package.json.
  const pkgPath = createRequire(join(dir, "package.json")).resolve("vite/package.json");
  const { bin } = JSON.parse(readFileSync(pkgPath, "utf8"));
  const vite = join(dirname(pkgPath), typeof bin === "string" ? bin : bin.vite);
  return spawn(process.execPath, [vite], { cwd: dir, stdio: "inherit" });
}

/**
 * Keep `port` served for as long as this process runs: start Vite in `dir`
 * when nothing answers there, and reuse whatever does (another dev loop's
 * Vite). If that server goes away — its dev loop was stopped — start ours.
 * Returns a function that stops the Vite this process started, if any.
 */
export async function keepServed({ name, dir, port }) {
  let child = null;
  let reusing = false;
  const check = async () => {
    if (child || (await isServed(port))) {
      if (!child && !reusing) {
        console.log(`[dev] ${name} UI already served on :${port}; reusing it`);
        reusing = true;
      }
      return;
    }
    if (reusing) console.log(`[dev] ${name} UI on :${port} went away; serving it here`);
    reusing = false;
    child = startVite(dir);
    // Lost the port to another dev loop starting at the same time, or Vite
    // stopped: forget it; the next check reuses or restarts.
    child.on("exit", () => {
      child = null;
    });
  };
  await check();
  const timer = setInterval(check, RECHECK_MS);
  return () => {
    clearInterval(timer);
    if (child && child.exitCode === null) child.kill("SIGTERM");
  };
}

/** Run `stops` when this process is stopped; until then, stay up. */
export function holdUntilStopped(stops) {
  const stop = () => {
    for (const s of stops) s();
    process.exit(0);
  };
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(signal, stop);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  holdUntilStopped([
    await keepServed({ name: "Terminal", dir: TERMINAL_DIR, port: TERMINAL_UI_PORT }),
  ]);
}

export { TERMINAL_DIR };
