// Starts the real Rust FDC3 bus (examples/fdc3_bus_harness.rs) for the contract
// tests. CARGO must have built it first: see the `test` npm script's error below.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { TestProject } from "vitest/node";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolve(typeof address === "object" && address ? address.port : 0));
    });
  });
}

let child: ChildProcess | undefined;

export default async function setup(project: TestProject) {
  const build = spawnSync("cargo", ["build", "-q", "-p", "desktop-agent", "--example", "fdc3_bus_harness"], {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (build.status !== 0) throw new Error("Could not build fdc3_bus_harness");

  const port = await freePort();
  const exe = process.platform === "win32" ? ".exe" : "";
  const binary = join(repoRoot, "target", "debug", "examples", `fdc3_bus_harness${exe}`);
  // stdout: the harness's launch/close protocol; stderr: its logs, including
  // the bus's "WebSocket server" line once it is listening.
  child = spawn(binary, [String(port)], { stdio: ["ignore", "pipe", "pipe"] });
  child.stdout!.resume();
  await new Promise<void>((resolve, reject) => {
    child!.once("error", reject);
    const timer = setTimeout(() => reject(new Error("fdc3_bus_harness did not start")), 10_000);
    // Accumulate: a pipe chunk can split the marker line.
    let seen = "";
    child!.stderr!.setEncoding("utf8");
    child!.stderr!.on("data", (chunk: string) => {
      if (seen.includes("WebSocket server")) return; // keep draining after start
      seen += chunk;
      if (seen.includes("WebSocket server")) {
        clearTimeout(timer);
        resolve();
      }
    });
  });
  project.provide("busUrl", `ws://127.0.0.1:${port}/fdc3`);
  return () => {
    child?.kill();
  };
}

declare module "vitest" {
  export interface ProvidedContext {
    busUrl: string;
  }
}
