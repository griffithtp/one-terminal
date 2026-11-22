#!/usr/bin/env node
/**
 * `tauri dev`'s beforeDevCommand for the Desktop Agent.
 *
 * The agent starts Terminals by running the `one-terminal` binary next to its
 * own (`target/debug/`), and a debug Terminal loads its UI from the Terminal's
 * Vite server (:1422). `tauri dev` builds and serves only the agent, so
 * without this a Terminal opened from the agent is whatever binary was built
 * last — possibly weeks old — or a blank window. This:
 *
 * 1. builds the Terminal (`cargo build -p one-terminal`; seconds when nothing
 *    changed),
 * 2. keeps the Terminal UI served, reusing another dev loop's server (e.g.
 *    `npm run dev:terminal`) while it runs,
 * 3. serves the agent's own UI (:1421), which `tauri dev` waits for.
 *
 * Terminal source changed while the agent runs? Restart `dev:desktop-agent`,
 * or run `npm run dev:terminal` alongside it.
 */
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  holdUntilStopped,
  keepServed,
  TERMINAL_DIR,
  TERMINAL_UI_PORT,
} from "../../one-terminal/scripts/dev-ui.mjs";

const AGENT_DIR = join(dirname(fileURLToPath(import.meta.url)), "..");
const AGENT_UI_PORT = 1421;

console.log("[dev] building the Terminal the Desktop Agent starts (cargo build -p one-terminal)");
const build = spawnSync("cargo", ["build", "-p", "one-terminal"], { stdio: "inherit" });
if (build.error || build.status !== 0) {
  console.error(
    `[dev] building one-terminal failed${build.error ? `: ${build.error.message}` : ""}; ` +
      "Terminals opened from the Desktop Agent would be stale or missing"
  );
  process.exit(build.status ?? 1);
}

holdUntilStopped([
  await keepServed({ name: "Terminal", dir: TERMINAL_DIR, port: TERMINAL_UI_PORT }),
  await keepServed({ name: "Desktop Agent", dir: AGENT_DIR, port: AGENT_UI_PORT }),
]);
