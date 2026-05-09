import * as p from "@clack/prompts";
import { resolve } from "node:path";
import { buildContext, type ScaffoldContext } from "./context.js";

export interface PromptsResult {
  ctx: ScaffoldContext;
  outputDir: string;
}

export async function runPrompts(): Promise<PromptsResult> {
  p.intro("OneTerminal Scaffolder");

  const workspaceName = await p.text({
    message: "Workspace name (kebab-case)",
    placeholder: "acme-trading",
    validate: (v) => {
      if (!/^[a-z][a-z0-9-]+$/.test(v)) return "Must be lowercase kebab-case (e.g. acme-trading)";
    },
  });
  if (p.isCancel(workspaceName)) cancel();

  const outputDirInput = await p.text({
    message: "Output folder",
    initialValue: `./${workspaceName as string}`,
    validate: (v) => {
      if (!v.trim()) return "Folder path cannot be empty";
    },
  });
  if (p.isCancel(outputDirInput)) cancel();

  const outputDir = resolve(outputDirInput as string);

  const tauriIdentifier = await p.text({
    message: "Reverse-domain identifier (used for Tauri bundle ID)",
    placeholder: "com.acme.trading",
    validate: (v) => {
      if (!/^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$/.test(v))
        return "Must be dot-separated lowercase segments (e.g. com.acme.trading)";
    },
  });
  if (p.isCancel(tauriIdentifier)) cancel();

  const includeFdc3 = await p.confirm({
    message: "Include FDC3 2.2 integration (ot-fdc3 / fdc3-client / fdc3-plugin)?",
    initialValue: true,
  });
  if (p.isCancel(includeFdc3)) cancel();

  const overridePorts = await p.confirm({
    message: "Customize default ports?",
    initialValue: false,
  });
  if (p.isCancel(overridePorts)) cancel();

  let portAnswers: {
    terminalDevPort?: number;
    agentDevPort?: number;
    tcpBrokerPort?: number;
    fdc3BusPort?: number;
    dacpBridgePort?: number;
    appDirectoryPort?: number;
  } = {};

  if (overridePorts) {
    const ports = await p.group({
      terminalDevPort: () =>
        p.text({
          message: "Terminal (one-terminal) Vite dev port",
          initialValue: "1422",
          validate: validatePort,
        }),
      agentDevPort: () =>
        p.text({
          message: "Desktop Agent Vite dev port",
          initialValue: "1421",
          validate: validatePort,
        }),
      tcpBrokerPort: () =>
        p.text({ message: "TCP broker port", initialValue: "7890", validate: validatePort }),
      fdc3BusPort: () =>
        p.text({
          message: "FDC3 bus WebSocket port",
          initialValue: "7891",
          validate: validatePort,
        }),
      dacpBridgePort: () =>
        p.text({ message: "DACP bridge port", initialValue: "4475", validate: validatePort }),
      appDirectoryPort: () =>
        p.text({ message: "App Directory port", initialValue: "3005", validate: validatePort }),
    });
    portAnswers = {
      terminalDevPort: Number(ports.terminalDevPort),
      agentDevPort: Number(ports.agentDevPort),
      tcpBrokerPort: Number(ports.tcpBrokerPort),
      fdc3BusPort: Number(ports.fdc3BusPort),
      dacpBridgePort: Number(ports.dacpBridgePort),
      appDirectoryPort: Number(ports.appDirectoryPort),
    };
  }

  const ctx = buildContext({
    workspaceName: workspaceName as string,
    tauriIdentifier: tauriIdentifier as string,
    includeFdc3: includeFdc3 as boolean,
    ...portAnswers,
  });

  p.note(
    [
      `Workspace:   ${ctx.workspaceName}`,
      `npm scope:   @${ctx.orgScope}/*`,
      `Tauri ID:    ${ctx.tauriIdentifier}.*`,
      `FDC3:        ${ctx.includeFdc3 ? "yes" : "no"}`,
      `Output:      ${outputDir}`,
    ].join("\n"),
    "Summary"
  );

  const go = await p.confirm({ message: "Generate workspace?", initialValue: true });
  if (p.isCancel(go) || !go) cancel();

  return { ctx, outputDir };
}

function validatePort(v: string): string | undefined {
  const n = Number(v);
  if (!Number.isInteger(n) || n < 1024 || n > 65535)
    return "Must be an integer between 1024 and 65535";
}

function cancel(): never {
  p.cancel("Cancelled.");
  process.exit(0);
}
