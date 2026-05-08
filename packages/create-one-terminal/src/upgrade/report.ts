import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { MigrationResult } from "./migrations/types.js";

export async function writeReport(
  cwd: string,
  fromVersion: string,
  toVersion: string,
  results: MigrationResult[]
): Promise<void> {
  const applied = results.filter((r) => r.status === "applied");
  const skipped = results.filter((r) => r.status === "skipped");
  const manual = results.filter((r) => r.status === "needs-manual");
  const failed = results.filter((r) => r.status === "failed");

  const lines: string[] = [
    "# OneTerminal Upgrade Report",
    "",
    `**Upgraded from:** v${fromVersion}`,
    `**Upgraded to:** v${toVersion}`,
    `**Date:** ${new Date().toISOString().slice(0, 10)}`,
    "",
  ];

  if (applied.length) {
    lines.push("## Applied", "");
    for (const r of applied) lines.push(`- \`${r.id}\` — ${r.description}`);
    lines.push("");
  }

  if (skipped.length) {
    lines.push("## Skipped (you chose to keep your values)", "");
    for (const r of skipped) lines.push(`- \`${r.id}\` — ${r.description}`);
    lines.push("");
  }

  if (manual.length) {
    lines.push("## Needs Manual Intervention", "");
    for (const r of manual) {
      lines.push(`- \`${r.id}\` — ${r.description}`);
      if (r.detail) lines.push(`  ${r.detail}`);
    }
    lines.push("");
  }

  if (failed.length) {
    lines.push("## Failed", "");
    for (const r of failed) {
      lines.push(`- \`${r.id}\` — ${r.description}`);
      if (r.detail) lines.push(`  Error: ${r.detail}`);
    }
    lines.push("");
  }

  const reportPath = join(".one-terminal", `upgrade-report-${toVersion}.md`);

  lines.push(
    "## Next Steps",
    "",
    "1. Resolve any `.merge` files.",
    "2. Run `cargo check --workspace` and `npm install` to verify.",
    `3. Delete \`${reportPath}\` when done.`
  );

  const reportFile = join(cwd, reportPath);
  await mkdir(join(cwd, ".one-terminal"), { recursive: true });
  await writeFile(reportFile, lines.join("\n"), "utf8");
}
