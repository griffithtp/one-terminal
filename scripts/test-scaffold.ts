#!/usr/bin/env tsx
/**
 * Smoke-tests the scaffolder by rendering a workspace to a temp directory
 * and running `cargo check --workspace` against it.
 *
 * Usage:
 *   npx tsx scripts/test-scaffold.ts
 *   npx tsx scripts/test-scaffold.ts --keep   # don't delete the output on success
 */

import { execSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const ROOT = join(fileURLToPath(import.meta.url), "../..");
const keep = process.argv.includes("--keep");

// Import the compiled scaffolder directly — bypasses the interactive prompts.
const { renderWorkspace } = await import(
  join(ROOT, "packages/create-one-terminal/dist/create/render.js")
);
const { buildContext } = await import(
  join(ROOT, "packages/create-one-terminal/dist/create/context.js")
);
const { createWidget } = await import(
  join(ROOT, "packages/create-one-terminal/dist/new-widget/index.js")
);

// CLI: `npx tsx scripts/test-scaffold.ts [--variant standalone|enterprise|both] [--keep]`
const variantArg = process.argv.indexOf("--variant");
const variantSelector =
  variantArg !== -1
    ? (process.argv[variantArg + 1] as "standalone" | "enterprise" | "both")
    : "both";

type Variant = "standalone" | "enterprise";
const variants: Variant[] =
  variantSelector === "both" ? ["standalone", "enterprise"] : [variantSelector as Variant];

const outputs: string[] = [];
let failure: Error | null = null;

try {
  for (const variant of variants) {
    const OUT = mkdtempSync(join(tmpdir(), `ot-scaffold-test-${variant}-`));
    outputs.push(OUT);
    console.log(`\n══ Scaffolding ${variant} → ${OUT}`);

    const ctx = buildContext({
      workspaceName: "test-workspace",
      tauriIdentifier: "com.test.workspace",
      variant,
      includeFdc3: true,
    });

    await renderWorkspace(ctx, OUT);

    // post-scaffold equivalent: write the oneTerminal metadata so new-widget
    // can detect the workspace variant.
    const pkgPath = join(OUT, "package.json");
    const pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as Record<string, unknown>;
    pkg.oneTerminal = {
      version: ctx.scaffoldVersion,
      scaffoldedAt: ctx.scaffoldedAt,
      variant: ctx.variant,
    };
    writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

    const fileCount = execSync(`find "${OUT}" -type f | wc -l`, { encoding: "utf8" }).trim();
    console.log(`✓ Rendered ${fileCount} files`);

    console.log("Running cargo check --workspace…");
    execSync("cargo check --workspace", { cwd: OUT, stdio: "inherit" });
    console.log("✓ cargo check passed");

    console.log("Running npm install…");
    execSync("npm install --ignore-scripts", { cwd: OUT, stdio: "inherit" });
    console.log("✓ npm install passed");

    if (variant === "enterprise") {
      console.log("Running npm run build:app-directory…");
      execSync("npm run build:app-directory", { cwd: OUT, stdio: "inherit" });
      console.log("✓ build:app-directory passed");
    }

    // Exercise new-widget against the scaffolded workspace.
    console.log("Running create-one-terminal new-widget…");
    await createWidget(OUT, {
      widgetName: "smoke-widget",
      widgetTitle: "Smoke Widget",
      widgetPort: 3099,
      orgScope: "test-workspace",
    });
    const widgetDir = join(OUT, "apps/smoke-widget");
    if (!existsSync(join(widgetDir, "server.js"))) {
      throw new Error("new-widget did not create apps/smoke-widget/server.js");
    }
    if (variant === "standalone") {
      const registry = JSON.parse(readFileSync(join(OUT, "widgets.config.json"), "utf8")) as {
        widgets: Array<{ appId: string }>;
      };
      if (!registry.widgets.some((w) => w.appId === "smoke-widget")) {
        throw new Error("widgets.config.json was not updated with smoke-widget");
      }
    }
    console.log("✓ new-widget produced apps/smoke-widget and registered it");

    console.log(`✓ ${variant}: all checks passed`);
  }
} catch (err) {
  failure = err as Error;
} finally {
  if (!keep) {
    for (const o of outputs) rmSync(o, { recursive: true, force: true });
    console.log("\n(temp dirs cleaned up — use --keep to retain them)");
  } else {
    console.log("\nKept outputs:", outputs.join(", "));
  }
}

if (failure) {
  console.error("\n✗ Test failed:", failure.message);
  process.exit(1);
}
console.log("\n✓ All variants passed.");
