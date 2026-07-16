export type Variant = "standalone" | "enterprise";

export interface ScaffoldContext {
  workspaceName: string;
  orgScope: string;
  tauriIdentifier: string;
  displayName: string;
  snakeWorkspaceName: string;
  variant: Variant;
  externalFdc3AgentUrl: string;
  terminalDevPort: number;
  agentDevPort: number;
  tcpBrokerPort: number;
  fdc3BusPort: number;
  dacpBridgePort: number;
  appDirectoryPort: number;
  includeFdc3: boolean;
  scaffoldVersion: string;
  scaffoldedAt: string;
}

export function buildContext(answers: {
  workspaceName: string;
  tauriIdentifier: string;
  includeFdc3: boolean;
  variant?: Variant;
  externalFdc3AgentUrl?: string;
  terminalDevPort?: number;
  agentDevPort?: number;
  tcpBrokerPort?: number;
  fdc3BusPort?: number;
  dacpBridgePort?: number;
  appDirectoryPort?: number;
}): ScaffoldContext {
  return {
    workspaceName: answers.workspaceName,
    orgScope: answers.workspaceName,
    tauriIdentifier: answers.tauriIdentifier,
    displayName: toPascalCase(answers.workspaceName),
    snakeWorkspaceName: toSnakeCase(answers.workspaceName),
    variant: answers.variant ?? "enterprise",
    externalFdc3AgentUrl: answers.externalFdc3AgentUrl ?? "",
    terminalDevPort: answers.terminalDevPort ?? 1422,
    agentDevPort: answers.agentDevPort ?? 1421,
    tcpBrokerPort: answers.tcpBrokerPort ?? 7890,
    fdc3BusPort: answers.fdc3BusPort ?? 7891,
    dacpBridgePort: answers.dacpBridgePort ?? 4475,
    appDirectoryPort: answers.appDirectoryPort ?? 3005,
    includeFdc3: answers.includeFdc3,
    scaffoldVersion: "0.1.11",
    scaffoldedAt: new Date().toISOString().slice(0, 10),
  };
}

export function toSnakeCase(s: string): string {
  return s.replace(/-/g, "_");
}

export function toPascalCase(s: string): string {
  return s
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}
