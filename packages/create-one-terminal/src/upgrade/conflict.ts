import * as p from "@clack/prompts";
import { conflictMarkers } from "../utils/diff.js";
import { writeFile } from "node:fs/promises";

export type ConflictChoice = "auto" | "skip" | "merge";

export async function handleConflict(opts: {
  label: string;
  filePath: string;
  fieldPath: string;
  yourValue: string;
  frameworkValue: string;
  frameworkVersion: string;
}): Promise<ConflictChoice> {
  p.note(
    [
      `File:   ${opts.filePath}`,
      `Field:  ${opts.fieldPath}`,
      `  Framework wants: ${opts.frameworkValue}`,
      `  Your value:      ${opts.yourValue}`,
    ].join("\n"),
    "Conflict",
  );

  const choice = await p.select<ConflictChoice>({
    message: "How should this conflict be resolved?",
    options: [
      { value: "auto", label: "Apply framework value (overwrite yours)" },
      { value: "skip", label: "Keep your value, skip this change" },
      { value: "merge", label: "Write .merge file for manual resolution" },
    ],
  });

  if (p.isCancel(choice)) {
    p.cancel("Upgrade cancelled.");
    process.exit(0);
  }

  if (choice === "merge") {
    const markers = conflictMarkers(
      `framework v${opts.frameworkVersion}`,
      `  "${opts.fieldPath}": ${opts.yourValue}`,
      `  "${opts.fieldPath}": ${opts.frameworkValue}`,
    );
    await writeFile(`${opts.filePath}.merge`, markers, "utf8");
  }

  return choice as ConflictChoice;
}
