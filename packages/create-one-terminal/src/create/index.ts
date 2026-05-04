import { runPrompts } from "./prompts.js";
import { renderWorkspace } from "./render.js";
import { postScaffold } from "./post-scaffold.js";

export async function runCreate(): Promise<void> {
  const { ctx, outputDir } = await runPrompts();
  await renderWorkspace(ctx, outputDir);
  await postScaffold(ctx, outputDir);
}
