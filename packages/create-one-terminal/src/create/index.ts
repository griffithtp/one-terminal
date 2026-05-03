import { resolve } from "node:path";
import { runPrompts } from "./prompts.js";
import { renderWorkspace } from "./render.js";
import { postScaffold } from "./post-scaffold.js";

export async function runCreate(outputDir?: string): Promise<void> {
  const dest = resolve(outputDir ?? process.argv[3] ?? process.cwd());
  const ctx = await runPrompts(dest);
  await renderWorkspace(ctx, dest);
  await postScaffold(ctx, dest);
}
