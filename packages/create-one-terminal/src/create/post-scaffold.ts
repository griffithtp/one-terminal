import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import * as p from "@clack/prompts";
import type { ScaffoldContext } from "./context.js";

export async function postScaffold(ctx: ScaffoldContext, outputDir: string): Promise<void> {
  const pkgPath = join(outputDir, "package.json");
  const raw = await readFile(pkgPath, "utf8");
  const pkg = JSON.parse(raw) as Record<string, unknown>;

  pkg["oneTerminal"] = {
    version: ctx.scaffoldVersion,
    scaffoldedAt: ctx.scaffoldedAt,
    variant: ctx.variant,
  };

  await writeFile(pkgPath, JSON.stringify(pkg, null, 2) + "\n", "utf8");

  const nextSteps =
    ctx.variant === "standalone"
      ? [
          "Start developing (two terminals):",
          "  npm run dev:sample-widget",
          "  npm run dev:terminal",
          "",
          "To register more widgets, edit widgets.config.json.",
        ]
      : [
          "Start developing (three terminals):",
          "  npm run dev:app-directory",
          "  npm run dev:desktop-agent",
          "  npm run dev:terminal",
        ];

  p.outro(
    [
      `Workspace created at: ${outputDir}  (variant: ${ctx.variant})`,
      "",
      "Next steps:",
      `  cd ${outputDir}`,
      "  cargo check --workspace",
      "  npm install",
      "",
      ...nextSteps,
    ].join("\n")
  );
}
