// Tests for scripts/check-docs.ts. Run: npm run test:scripts
import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  anchors,
  checkAgentIndex,
  checkEnvVars,
  checkLayout,
  checkLink,
  extractCommentLinks,
  extractMarkdownLinks,
  findEnvReads,
  findPlanReferences,
  normalizeText,
  parseEnvTables,
  parseLayout,
  type Finding,
  type Repo,
} from "./check-docs.js";

const AGENTS_MD = `# Root

## Workspace layout

\`\`\`
Cargo workspace root: Cargo.toml
apps/one-terminal/            — Terminal
apps/desktop-agent/src-tauri  — Desktop Agent
packages/ot-core/             — shared crate
\`\`\`

## Environment variables

| Variable           | Component     | Purpose   |
| ------------------ | ------------- | --------- |
| \`OT_FDC3_BUS_PORT\` | Desktop Agent | Bus port  |
| \`PORT\`             | App Directory | Listen    |

### Internal variables

| Variable | Set by | Purpose |
| -------- | ------ | ------- |
| \`OT_URL\` | hosts  | URL     |

Not yet implemented (planned): \`OT_NATIVE_APP_ALLOW_PATHS\`.

## Next section

| \`OT_NOT_ENV\` | outside the section |
`;

const names = (findings: { message: string }[]) => findings.map((f) => f.message);

/** The message of a finding that must exist. */
function message(finding: Finding | null | undefined): string {
  assert.ok(finding, "expected a finding");
  return finding.message;
}

describe("env vars", () => {
  const documented = parseEnvTables(AGENTS_MD);

  it("parses both tables in the section, and nothing outside it", () => {
    assert.deepEqual([...documented.keys()], ["OT_FDC3_BUS_PORT", "PORT", "OT_URL"]);
  });

  it("passes when every read is documented and every OT_ row is read", () => {
    const reads = [
      ...findEnvReads("a.rs", `let p = std::env::var("OT_FDC3_BUS_PORT");`),
      ...findEnvReads("main.js", `const u = process.env.OT_URL;`),
    ];
    assert.deepEqual(checkEnvVars(reads, documented), []);
  });

  it("reports a documented OT_ variable that nothing reads (a removed read)", () => {
    const reads = findEnvReads("a.rs", `std::env::var("OT_FDC3_BUS_PORT")`);
    const messages = names(checkEnvVars(reads, documented));
    assert.equal(messages.length, 1);
    assert.match(messages[0], /OT_URL is documented but no source reads it/);
  });

  it("reports a read whose table row was removed", () => {
    const withoutRow = new Map(documented);
    withoutRow.delete("OT_FDC3_BUS_PORT");
    const reads = [
      ...findEnvReads("a.rs", `std::env::var("OT_FDC3_BUS_PORT")`),
      ...findEnvReads("b.js", `process.env.OT_URL`),
    ];
    const findings = checkEnvVars(reads, withoutRow);
    assert.deepEqual(
      findings.map((f) => `${f.file}:${f.line}`),
      ["a.rs:1"]
    );
  });

  it("reports an undocumented literal read with its line", () => {
    const reads = findEnvReads("x.rs", `fn f() {}\nlet v = std::env::var("OT_DUMMY");\n`);
    assert.deepEqual(reads, [{ name: "OT_DUMMY", file: "x.rs", line: 2 }]);
    assert.match(names(checkEnvVars(reads, documented)).join(), /OT_DUMMY is read here/);
  });

  it("finds var_os reads", () => {
    const reads = findEnvReads("x.rs", `std::env::var_os("OT_DUMMY").is_some()`);
    assert.deepEqual(
      reads.map((r) => r.name),
      ["OT_DUMMY"]
    );
  });

  it("resolves a read through a const", () => {
    const src = `const X: &str = "OT_DUMMY";\n\nfn f() { let _ = std::env::var(X); }\n`;
    assert.deepEqual(findEnvReads("x.rs", src), [{ name: "OT_DUMMY", file: "x.rs", line: 3 }]);
  });

  it("finds process.env reads in .js, dotted and bracketed", () => {
    const src = `const a = process.env.OT_DUMMY;\nconst b = process.env["OT_OTHER"];`;
    assert.deepEqual(
      findEnvReads("main.js", src).map((r) => r.name),
      ["OT_DUMMY", "OT_OTHER"]
    );
  });

  it("ignores identifiers that merely contain OT_", () => {
    const src = `.status(StatusCode::NOT_FOUND);\nconst SNAPSHOT_EVENT = "wm:x";\nlet x = LISTENER_NOT_FOUND;`;
    assert.deepEqual(findEnvReads("x.rs", src), []);
    assert.deepEqual(findEnvReads("x.ts", src), []);
  });

  it("ignores reads in Rust test modules and test files", () => {
    const src = `fn f() {}\n#[cfg(test)]\nmod tests { fn t() { std::env::var("OT_TEST_ONLY"); } }\n`;
    assert.deepEqual(findEnvReads("x.rs", src), []);
    assert.deepEqual(findEnvReads("x.test.ts", `process.env.OT_TEST_ONLY`), []);
  });

  it("doesn't reverse-check non-OT_ rows such as PORT", () => {
    const reads = [
      ...findEnvReads("a.rs", `std::env::var("OT_FDC3_BUS_PORT")`),
      ...findEnvReads("b.js", `process.env.OT_URL`),
    ];
    assert.ok(!names(checkEnvVars(reads, documented)).some((m) => m.includes("PORT is")));
  });
});

describe("workspace layout", () => {
  const listed = parseLayout(AGENTS_MD);
  const exists = (p: string) => listed.has(p);

  it("parses only apps/ and packages/ paths, without trailing slashes", () => {
    assert.deepEqual(
      [...listed.keys()],
      ["apps/one-terminal", "apps/desktop-agent/src-tauri", "packages/ot-core"]
    );
  });

  it("counts a directory as covered when a path under it is listed", () => {
    const dirs = ["apps/one-terminal", "apps/desktop-agent", "packages/ot-core"];
    assert.deepEqual(checkLayout(listed, dirs, exists), []);
  });

  it("reports an unlisted directory", () => {
    const findings = checkLayout(listed, ["apps/new-thing"], exists, 3);
    assert.deepEqual(
      findings.map((f) => [f.line, f.message]),
      [[3, "apps/new-thing/ is not covered by the Workspace layout block"]]
    );
  });

  it("reports a listed path that doesn't exist", () => {
    const findings = checkLayout(listed, [], (p) => p !== "packages/ot-core");
    assert.match(names(findings).join(), /layout lists packages\/ot-core\/, which doesn't exist/);
  });
});

describe("agents index", () => {
  it("reports agents and skills missing from the index tables", () => {
    const agentsMd = "[a](.ai/agents/listed.md) [s](.ai/skills/listed/SKILL.md)";
    const aiReadme = "[a](agents/listed.md)";
    const findings = checkAgentIndex(["listed", "new"], ["listed", "flow"], agentsMd, aiReadme);
    assert.deepEqual(names(findings), [
      "new is not in the AGENTS.md agents table",
      "new is not in the .ai/README.md agents table",
      "flow is not in the AGENTS.md workflow table",
    ]);
  });
});

describe("plan references", () => {
  it("reports a plan number in a code comment", () => {
    const findings = findPlanReferences("src/a.rs", "fn f() {}\n// see Plan 12\n");
    assert.deepEqual(
      findings.map((f) => f.line),
      [2]
    );
  });

  it("reports docs/plans/ paths, moved docs and .work/ links", () => {
    const text =
      "docs/plans/01-x.md\ndocs/dashboards/a.md\nTerminal Menu.md\n.work/plans/x.md\nEpic 08\n";
    assert.equal(findPlanReferences("README.md", text).length, 5);
  });

  it("lets the planning-folder docs name .work/ but not a plan number", () => {
    assert.deepEqual(findPlanReferences("AGENTS.md", "plans live in `.work/plans/`"), []);
    assert.equal(findPlanReferences("AGENTS.md", "see Plan 15").length, 1);
  });

  it("exempts the checker, its tests, versions.json and .gitignore", () => {
    const text = "docs/plans/ Plan 12 .work/";
    for (const file of [
      "scripts/check-docs.ts",
      "scripts/check-docs.test.ts",
      "packages/create-one-terminal/versions.json",
      ".gitignore",
    ]) {
      assert.deepEqual(findPlanReferences(file, text), [], file);
    }
  });

  it("doesn't match ordinary words or one-digit phases", () => {
    assert.deepEqual(
      findPlanReferences("a.md", "Planned work. Phase 9. Plan 7. Explanation 12"),
      []
    );
  });
});

describe("links", () => {
  const files: Record<string, string> = {
    "README.md": "# Title\n\n## 🧩 Widget UI Extension Points\n\n## Build / check commands\n",
    "docs/guide.md": "# Guide\n\n## Setup\n\n## Setup\n\n### Conditional lines (`ot:if`)\n",
    "docs/conventions/rust.md": "# Rust\n",
    "apps/one-terminal/src/components/WidgetCatalog.tsx": "",
    "src/lib.rs": "",
  };
  const repo: Repo = {
    files: new Set(Object.keys(files)),
    dirs: new Set(["docs", "docs/conventions", "apps", "apps/one-terminal", "src"]),
    read: (p) => files[p] ?? "",
  };
  const check = (from: string, target: string) => checkLink(from, { target, line: 7 }, repo);

  it("accepts existing files, directories, headings and line links", () => {
    for (const target of [
      "docs/guide.md",
      "docs/",
      "docs/guide.md#setup",
      "docs/guide.md#setup-1",
      "docs/guide.md#conditional-lines-otif",
      "#-widget-ui-extension-points",
      "#build--check-commands",
      "src/lib.rs#L10-L20",
      "https://example.com/x",
      "mailto:a@b.c",
    ]) {
      assert.equal(check("README.md", target), null, target);
    }
  });

  it("reports a missing file with its line", () => {
    assert.deepEqual(check("README.md", "docs/missing.md"), {
      file: "README.md",
      line: 7,
      message: "docs/missing.md: no such file",
    });
  });

  it("reports a case mismatch", () => {
    assert.match(
      message(check("README.md", "docs/Conventions/rust.md")),
      /case mismatch, the file is docs\/conventions\/rust.md/
    );
  });

  it("reports a missing heading anchor", () => {
    assert.match(message(check("README.md", "docs/guide.md#install")), /no heading #install/);
    assert.match(message(check("README.md", "#widget-ui-extension-points")), /no heading/);
  });

  it("reports a malformed anchor instead of crashing", () => {
    assert.match(message(check("README.md", "docs/guide.md#100%")), /malformed anchor/);
  });

  it("reports links that leave the repo or point into .work/", () => {
    assert.match(message(check("README.md", "../../issues")), /outside the repository/);
    assert.match(message(check("docs/guide.md", "../.work/plans/x.md")), /into \.work\//);
  });

  it("numbers duplicate headings like GitHub", () => {
    assert.deepEqual(
      [...anchors("# A\n## Setup\n## Setup\n## Setup\n")],
      ["a", "setup", "setup-1", "setup-2"]
    );
  });

  it("ignores links in fenced code blocks and inline code", () => {
    const md = "[real](a.md)\n\n```md\n[fenced](b.md)\n```\n\nUse `[inline](c.md)` here.\n";
    assert.deepEqual(extractMarkdownLinks(md), [{ target: "a.md", line: 1 }]);
  });

  it("extracts HTML href links", () => {
    assert.deepEqual(extractMarkdownLinks('<a href="CONTRIBUTING.md"><b>x</b></a>'), [
      { target: "CONTRIBUTING.md", line: 1 },
    ]);
  });

  it("reports a broken link in a TSX doc comment, not one in a string literal", () => {
    const src = `/**\n * See [Catalog](./WidgetCatalog.tsx) and [gone](./Missing.tsx).\n */\nconst s = "[x](./AlsoMissing.tsx)";\n`;
    const from = "apps/one-terminal/src/components/Picker.tsx";
    const links = extractCommentLinks(from, src);
    assert.deepEqual(
      links.map((l) => l.target),
      ["./WidgetCatalog.tsx", "./Missing.tsx"]
    );
    const broken = links.map((l) => checkLink(from, l, repo)).filter(Boolean);
    assert.equal(broken.length, 1);
    assert.match(message(broken[0]), /Missing\.tsx: no such file/);
  });

  it("ignores Rust intra-doc links but checks path links in doc comments", () => {
    const src =
      "//! See [`Registry`](crate::layout::Registry).\n/// Spec: [doc](../docs/guide.md)\nfn f() {}\n";
    assert.deepEqual(
      extractCommentLinks("src/lib.rs", src).map((l) => l.target),
      ["../docs/guide.md"]
    );
  });
});

describe("text normalisation", () => {
  it("turns CRLF into LF and strips a BOM, so Windows checkouts parse the same", () => {
    const crlf = "﻿## Environment variables\r\n\r\n| `OT_X` | a | b |\r\n";
    const text = normalizeText(crlf);
    assert.equal(text, "## Environment variables\n\n| `OT_X` | a | b |\n");
    assert.deepEqual([...parseEnvTables(text).keys()], ["OT_X"]);
    assert.deepEqual(
      extractMarkdownLinks(normalizeText("[a](b.md)\r\n")).map((l) => l.target),
      ["b.md"]
    );
  });
});
