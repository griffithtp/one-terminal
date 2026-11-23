#!/usr/bin/env node
/**
 * A local OpenTelemetry backend for OneTerminal: Grafana's all-in-one
 * `grafana/otel-lgtm` image (OTLP collector on :4318, Grafana on :3000) with
 * the OneTerminal dashboards from `observability/grafana/dashboards/`.
 *
 *   npm run otel:up       start it in the background and import the dashboards
 *   npm run otel:import   (re)import the dashboards into a Grafana already
 *                         running on GRAFANA_URL (default http://localhost:3000)
 *   npm run otel:down     stop it
 *
 * The container is removed when it stops, and so is its data unless `otel:up`
 * is given `--keep-data` (the `one-terminal-otel-lgtm` Docker volume). The
 * dashboards always come back from the repo: edit the JSON here, not in
 * Grafana — changes made in Grafana are lost on the next import.
 *
 * Dashboards are imported over Grafana's HTTP API rather than bind-mounted
 * into its provisioning folder: bind mounts of repo files fail on macOS when
 * Docker lacks access to the folder (e.g. ~/Documents), and on some Windows
 * setups.
 */
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Pinned: the dashboards use its data source uids (`loki`, `prometheus`,
// `tempo`) and anonymous admin access.
const IMAGE = "grafana/otel-lgtm:0.41.1";
const CONTAINER = "one-terminal-otel-lgtm";
const DASHBOARDS = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "observability",
  "grafana",
  "dashboards"
);
const GRAFANA_URL = (process.env.GRAFANA_URL ?? "http://localhost:3000").replace(/\/$/, "");
const FOLDER = { uid: "one-terminal", title: "OneTerminal" };

function docker(args, { quiet = false } = {}) {
  const result = spawnSync("docker", args, { encoding: "utf8", stdio: quiet ? "pipe" : "inherit" });
  if (result.error) {
    console.error(`could not run docker: ${result.error.message}`);
    process.exit(1);
  }
  return result;
}

async function grafana(path, init = {}) {
  const response = await fetch(`${GRAFANA_URL}${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...init.headers },
  });
  const body = await response.text();
  return { ok: response.ok, status: response.status, body };
}

async function waitForGrafana(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if ((await grafana("/api/health")).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((resolve) => setTimeout(resolve, 2_000));
  }
  throw new Error(`Grafana didn't answer on ${GRAFANA_URL} within ${timeoutMs / 1000} s`);
}

async function importDashboards() {
  const folder = await grafana("/api/folders", { method: "POST", body: JSON.stringify(FOLDER) });
  // 409/412: the folder exists already.
  if (!folder.ok && ![409, 412].includes(folder.status)) {
    throw new Error(
      `creating the ${FOLDER.title} folder failed (${folder.status}): ${folder.body}`
    );
  }
  const files = readdirSync(DASHBOARDS).filter((f) => f.endsWith(".json"));
  for (const file of files) {
    const dashboard = JSON.parse(readFileSync(join(DASHBOARDS, file), "utf8"));
    delete dashboard.id;
    const result = await grafana("/api/dashboards/db", {
      method: "POST",
      body: JSON.stringify({ dashboard, folderUid: FOLDER.uid, overwrite: true }),
    });
    if (!result.ok) throw new Error(`importing ${file} failed (${result.status}): ${result.body}`);
    const { url } = JSON.parse(result.body);
    console.log(`imported ${file} → ${GRAFANA_URL}${url}`);
  }
}

async function up() {
  const running = docker(["ps", "-q", "--filter", `name=^${CONTAINER}$`], { quiet: true });
  if (running.stdout.trim()) {
    console.log(`${CONTAINER} is already running`);
  } else {
    const args = ["run", "-d", "--rm", "--name", CONTAINER, "-p", "3000:3000", "-p", "4318:4318"];
    if (process.argv.includes("--keep-data")) args.push("-v", `${CONTAINER}:/data`);
    args.push(IMAGE);
    if (docker(args).status !== 0) process.exit(1);
  }
  console.log("waiting for Grafana…");
  await waitForGrafana(180_000);
  await importDashboards();
  console.log(
    `\nGrafana:  ${GRAFANA_URL}  (Dashboards → OneTerminal)\n` +
      "OTLP:     http://localhost:4318  — e.g. OT_OTLP_ENDPOINT=http://localhost:4318 npm run dev:desktop-agent\n" +
      "Stop it:  npm run otel:down"
  );
}

const command = process.argv[2];
try {
  if (command === "up") await up();
  else if (command === "import") {
    await waitForGrafana(10_000);
    await importDashboards();
  } else if (command === "down") process.exit(docker(["stop", CONTAINER]).status ?? 1);
  else {
    console.error("usage: node scripts/otel.mjs up [--keep-data] | import | down");
    process.exit(2);
  }
} catch (e) {
  console.error(e instanceof Error ? e.message : e);
  process.exit(1);
}
