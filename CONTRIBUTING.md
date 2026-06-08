# Contributing to OneTerminal

This guide covers how the framework is structured for contribution, with a focus on the scaffolding system — the part most likely to need maintenance as the framework evolves.

---

## Table of Contents

- [Development Setup](#development-setup)
- [Repo Layout](#repo-layout)
- [Making Framework Changes](#making-framework-changes)
  - [Step 1 — Edit the live app](#step-1--edit-the-live-app)
  - [Step 2 — Run the migration wizard](#step-2--run-the-migration-wizard)
  - [Step 3 — Build and test](#step-3--build-and-test)
  - [Step 4 — Open a PR](#step-4--open-a-pr)
- [The Template System](#the-template-system)
  - [How templates are generated](#how-templates-are-generated)
  - [Substitution table](#substitution-table)
  - [Conditional lines (`ot:if`)](#conditional-lines-otif)
  - [Static and binary files](#static-and-binary-files)
- [Updating the Scaffolder](#updating-the-scaffolder)
  - [The `create/` module](#the-create-module)
  - [The `upgrade/` module](#the-upgrade-module)
  - [The `merge/` module](#the-merge-module)
- [Migration Reference](#migration-reference)
  - [`config-merge`](#config-merge)
  - [`dep-bump`](#dep-bump)
  - [`structural`](#structural)
- [Testing Changes Locally](#testing-changes-locally)
  - [Test the framework itself](#test-the-framework-itself)
  - [Test a scaffolded workspace](#test-a-scaffolded-workspace)
  - [Test the upgrade path](#test-the-upgrade-path)
- [Publishing a Release](#publishing-a-release)
- [CI Workflows](#ci-workflows)
- [Naming Conventions](#naming-conventions)

---

## Development Setup

### Prerequisites

| Tool                          | Version                               |
| ----------------------------- | ------------------------------------- |
| [Rust](https://rustup.rs)     | stable ≥ 1.77                         |
| [Node.js](https://nodejs.org) | ≥ 20 LTS                              |
| Xcode Command Line Tools      | macOS only — `xcode-select --install` |

### Install dependencies

```sh
npm install
```

### Verify everything builds

```sh
cargo check --workspace
npm run build:scaffolder
```

---

## Repo Layout

```
one-terminal/
├── apps/                              Source-of-truth app code
│   ├── one-terminal/                  Window manager (Tauri 2) — shared overlay
│   ├── desktop-agent/                 FDC3 broker + engine launcher (Tauri 2) — enterprise
│   ├── tauri-webview-host/            Thin host for pinned WebView2/WKWebView — enterprise
│   ├── electron-host/                 Thin Electron host (optional, requires setup) — enterprise
│   ├── app-directory/                 FDC3 AppD REST API + management UI — enterprise
│   ├── sample-ticker/                 Demo browser app — broadcasts fx.rate context — enterprise
│   ├── sample-chart/                  Demo browser app — handles ViewChart intent — enterprise
│   └── sample-widget/                 Minimal demo widget — standalone variant
├── packages/
│   ├── ot-core/                       Shared Rust crate
│   ├── ot-fdc3/                       Tauri FDC3 spoke plugin — enterprise
│   ├── fdc3-plugin/                   Browser FDC3 client JS
│   ├── fdc3-client/                   TypeScript FDC3 types — enterprise (Tauri-bound)
│   └── create-one-terminal/           npm create scaffolder + upgrade tool
│       ├── src/
│       │   ├── create/                scaffold new workspace
│       │   ├── new-widget/            `new-widget` subcommand (wrapped by `npm run create-widget` in scaffolded workspaces)
│       │   ├── upgrade/               upgrade an existing workspace
│       │   └── merge/                 JSON + TOML merge helpers
│       ├── templates/                 ← GENERATED, never hand-edited
│       │   ├── shared/                files common to all variants
│       │   ├── enterprise/            full-stack-only files
│       │   └── standalone/            slim-variant-only files
│       ├── overlay-overrides/         hand-authored variant-specific files
│       │   └── standalone/            ← only place under templates that is hand-authored
│       ├── widget-template/           embedded template for `new-widget`
│       └── versions.json              upgrade migration manifest
├── scripts/
│   ├── extract-templates.ts           derives EJS templates from apps/ + copies overlay-overrides/
│   ├── create-migration.ts            interactive wizard: extract + diff + author migrations
│   ├── check-template-drift.ts        CI drift check
│   └── test-scaffold.ts               smoke-tests both variants end-to-end
└── .github/workflows/
    ├── template-drift.yml             blocks PRs with stale templates
    └── publish-scaffolder.yml         publishes on v* tags
```

**Key rule:** `packages/create-one-terminal/templates/` is always derived by the extractor script. Almost everything in there comes from `apps/` and `packages/ot-*/` via substitution. The one exception is `templates/standalone/Cargo.toml.ejs`, `templates/standalone/package.json.ejs`, `templates/standalone/widgets.config.json.ejs`, and `templates/standalone/apps/one-terminal/.../terminal.config.json.ejs` — these have no live source equivalent and are hand-authored under `packages/create-one-terminal/overlay-overrides/standalone/`. The extractor copies that directory verbatim. Edit there, never inside `templates/` — the CI drift check will reject hand edits to `templates/` directly.

### Variants and overlays

The scaffolder produces one of two workspace shapes selected by a prompt:

- **Standalone** _(default)_ — Terminal + a single sample widget + a local `widgets.config.json` widget registry. No Desktop Agent. The Terminal connects FDC3 to an external agent over a WebSocket URL.
- **Enterprise** — the full stack: Terminal + Desktop Agent + App Directory + engine plugin runtime + sample ticker/chart.

Templates are partitioned across three overlays. `render.ts` walks `shared/` first, then the selected variant overlay; collisions are won by the later overlay.

| Overlay       | Contents                                                                                                                                                   | Source                                                                                                                                                           |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `shared/`     | Terminal shell, ot-core, fdc3-plugin, root `.gitignore`                                                                                                    | Extracted from `apps/one-terminal/`, `packages/ot-core/`, `packages/fdc3-plugin/`                                                                                |
| `enterprise/` | Desktop Agent, App Directory, tauri-webview-host, electron-host, sample-ticker, sample-chart, ot-fdc3, fdc3-client, root `Cargo.toml`, root `package.json` | Extracted from the corresponding `apps/` / `packages/` dirs. `fdc3-client` is here because every method invokes `fdc3_*` Tauri commands registered by `ot-fdc3`. |
| `standalone/` | sample-widget, slim Cargo.toml, slim package.json, widgets.config.json, standalone terminal.config.json                                                    | sample-widget extracted from `apps/sample-widget/`; the rest from `overlay-overrides/standalone/`                                                                |

---

## Making Framework Changes

The apps in `apps/` are the canonical source. When you change app code, the EJS scaffolding templates must be regenerated so that `npm create one-terminal` produces a workspace that reflects your changes.

The `create-migration` wizard handles template extraction, diff analysis, migration authoring, version bumping, and `context.ts` updates in a single interactive session. Manual edits to `versions.json`, `context.ts`, or the templates directory are no longer needed for the common upgrade path.

### Step 1 — Edit the live app

Work directly in `apps/one-terminal/`, `apps/desktop-agent/`, `packages/ot-core/`, etc. as normal. If a new config key, dependency, or file is being introduced that scaffolded projects should receive, make the change in the live app first.

If the new value should be user-customisable at scaffold time (e.g. a port number), you must also:

1. Add a field to `ScaffoldContext` in [packages/create-one-terminal/src/create/context.ts](packages/create-one-terminal/src/create/context.ts)
2. Add a prompt in [packages/create-one-terminal/src/create/prompts.ts](packages/create-one-terminal/src/create/prompts.ts)
3. Add the substitution pair in `SUBSTITUTIONS` in [scripts/extract-templates.ts](scripts/extract-templates.ts) — longest/most-specific literal first (see [Substitution table](#substitution-table))

Do those scaffolder-side changes before running the wizard so they are reflected in the extracted templates.

### Step 2 — Run the migration wizard

```sh
npm run create-migration
```

The wizard runs end-to-end:

1. **Extracts templates** to a temp directory — same logic as `npm run extract-templates`
2. **Diffs** the fresh extraction against the committed templates in `packages/create-one-terminal/templates/`
3. **Reports changed files** and classifies each automatically:
   - `Cargo.toml` / `package.json` → `dep-bump` (auto-detected crate and npm version changes)
   - `*.json` config files → `config-merge` (deep diff with auto-filled patch)
   - Everything else → `structural` (shows a unified diff, prompts for anchor pattern and insertion content)
4. **Prompts for version bump** — patch / minor / major, with the resulting semver shown
5. **Writes** the new `versions.json` entry (prepended to the `versions` array)
6. **Updates** `scaffoldVersion` in `context.ts` to the new version
7. **Copies** the fresh templates to `packages/create-one-terminal/templates/`

At the end, review what was generated:

```sh
git diff packages/create-one-terminal/templates/
git diff packages/create-one-terminal/versions.json
git diff packages/create-one-terminal/src/create/context.ts
```

Then commit everything together:

```sh
git add apps/ packages/ot-core/                          # your app changes
git add packages/create-one-terminal/templates/
git add packages/create-one-terminal/versions.json
git add packages/create-one-terminal/src/create/context.ts
git commit -m "feat: <describe change>"
```

> **When no migration is needed** (pure internal refactor with no user-visible surface): decline all migration prompts in the wizard, or run `npm run extract-templates` directly and skip `create-migration`. In that case, manually bump `scaffoldVersion` in `context.ts` only if scaffolded projects need to be made aware of the change.

### Step 3 — Build and test

```sh
npm run build:scaffolder
```

Then follow [Test a scaffolded workspace](#test-a-scaffolded-workspace) to confirm the scaffolded output builds and runs. If you added a migration, also follow [Test the upgrade path](#test-the-upgrade-path).

### Step 4 — Open a PR

The CI drift check will run automatically and confirm templates are in sync. Once merged, a release is cut by pushing a `v*` tag (see [Publishing a Release](#publishing-a-release)).

---

## The Template System

### How templates are generated

`scripts/extract-templates.ts` walks each source tree in the `SOURCES` array, applies an ordered substitution table to text files, and writes the results as `.ejs` files into `packages/create-one-terminal/templates/<overlay>/...`. Each `SOURCES` entry declares which overlay (`shared`, `enterprise`, or `standalone`) its files belong to. After processing live sources, the extractor copies every file under `packages/create-one-terminal/overlay-overrides/<overlay>/` verbatim into the matching `templates/<overlay>/` directory — these are the hand-authored files (slim Cargo.toml etc.) that have no live source equivalent.

When `npm create one-terminal` runs, `packages/create-one-terminal/src/create/render.ts` walks the overlays in order (`shared/` first, then `<variant>/`), renders each `.ejs` file through EJS with a `ScaffoldContext` object, and writes the output to the new workspace directory. When the same destination path appears in both overlays, the variant overlay wins.

**EJS was chosen over Handlebars** because Rust source files, TOML, and Tauri JSON configs all use `{{` / `}}` extensively. EJS uses `<%= %>` delimiters which have no conflict.

### Substitution table

The substitution table in [scripts/extract-templates.ts](scripts/extract-templates.ts) maps literal strings in the source apps to EJS expressions. Entries are applied in order — more-specific strings must come before less-specific ones.

Example entries:

| Literal in source app            | EJS in template                           |
| -------------------------------- | ----------------------------------------- |
| `com.one-terminal.desktop-agent` | `<%= tauriIdentifier %>.agent`            |
| `com.one-terminal`               | `<%= tauriIdentifier %>`                  |
| `@one-terminal/desktop-agent`    | `@<%= orgScope %>/desktop-agent`          |
| `http://localhost:1422`          | `http://localhost:<%= terminalDevPort %>` |
| `"port": 1421`                   | `"port": <%= agentDevPort %>`             |

To add a new substitutable value:

1. Add a `ScaffoldContext` field in `context.ts`
2. Add a prompt in `prompts.ts` (or derive it automatically in `buildContext()`)
3. Add the substitution pair in `SUBSTITUTIONS` in `extract-templates.ts` — longest/most-specific literal first
4. Run `npm run extract-templates` to verify the output looks correct

### Conditional lines (`ot:if`)

A source line annotated with `# ot:if varName` (shell/TOML) or `// ot:if varName` (JS/Rust/JSON) is wrapped in an EJS `if` block during extraction.

**Source line:**

```toml
ot-fdc3 = { workspace = true }  # ot:if includeFdc3
```

**Generated template:**

```ejs
<% if (includeFdc3) { %>
ot-fdc3 = { workspace = true }
<% } %>
```

The variable name after `ot:if` must match a boolean field on `ScaffoldContext` (e.g. `includeFdc3`). This is the only inline mechanism for conditional template content — do not add EJS `if` blocks directly to source files.

For variant-level gating (a whole file that only exists in one variant), use the overlay system instead: put the file's live source under an enterprise-only `apps/<name>/` directory and tag it `overlay: "enterprise"` in `SOURCES`, or hand-author it under `overlay-overrides/standalone/`. Don't try to gate whole files with `ot:if`.

### Static and binary files

Some files are always copied verbatim (never templated):

- **Binary extensions** — `.png`, `.ico`, `.icns`, `.svg`
- **Static filenames** — `build.rs`, `vite-env.d.ts`, `.gitkeep`

To add a file that should always be copied unchanged, add its filename to the `STATIC_FILENAMES` set in [scripts/extract-templates.ts](scripts/extract-templates.ts).

### Stripping scripts from the root `package.json`

The root `package.json` in the framework repo contains scripts that are only meaningful when working on the framework itself (`extract-templates`, `check-template-drift`, `create-migration`, `build:scaffolder`, `scaffold`, `test:scaffold`) or only belong to one variant (`dev:sample-widget` belongs in the Standalone overlay-override, not the extracted Enterprise template). These are stripped from the extracted Enterprise template by `ROOT_PACKAGE_SCRIPTS_OMIT` in [scripts/extract-templates.ts](scripts/extract-templates.ts). The Standalone `package.json` is hand-authored in `overlay-overrides/standalone/` and ships only the scripts it actually needs, so the omit list does not affect it.

If you add a new framework-internal script to the root `package.json` that should not appear in scaffolded Enterprise workspaces, add its key to `ROOT_PACKAGE_SCRIPTS_OMIT`. Conversely, app-level scripts (`dev:*`, `build:*`) that belong in every Enterprise workspace should remain.

---

## Updating the Scaffolder

### The `create/` module

| File               | Responsibility                                                                                                        |
| ------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `context.ts`       | `ScaffoldContext` type (including `variant`, `externalFdc3AgentUrl`) + `buildContext()` derivations                   |
| `prompts.ts`       | Interactive CLI prompts using `@clack/prompts` — variant prompt drives which downstream prompts run                   |
| `render.ts`        | Walks `templates/shared/` then `templates/<variant>/`, renders `.ejs` files, writes atomically (variant-overlay-wins) |
| `post-scaffold.ts` | Injects `oneTerminal.{version,variant}` into generated `package.json`, prints variant-aware next-step instructions    |

### The `new-widget/` module

| File       | Responsibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `index.ts` | Interactive `new-widget` subcommand + programmatic `createWidget(cwd, spec)` API used by tests. Renders the embedded template at `packages/create-one-terminal/widget-template/`, then registers the widget — appends to `widgets.config.json` for Standalone workspaces, or prints/runs a `curl POST /v2/apps` for Enterprise. Scaffolded workspaces wrap this command behind an `npm run create-widget` script and ship `create-one-terminal` as an `optionalDependency` so the bin resolves locally. |

### The `upgrade/` module

| File                  | Responsibility                                                                                                             |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `index.ts`            | Orchestrates the 9-step upgrade flow                                                                                       |
| `detect.ts`           | Reads `oneTerminal.{version,variant}` from target `package.json`; infers variant from filesystem for pre-variant scaffolds |
| `manifest.ts`         | Fetches `versions.json` from the installed package                                                                         |
| `chain.ts`            | Builds the ordered list of migrations to apply, filtered by the project's variant (via each migration's `appliesTo`)       |
| `backup.ts`           | Snapshots affected files to `.one-terminal/upgrade-backup-<ver>/`                                                          |
| `restore.ts`          | Restores snapshot on failure                                                                                               |
| `conflict.ts`         | Handles `_managed`-field conflicts: auto / skip / merge                                                                    |
| `report.ts`           | Writes `.one-terminal/upgrade-report-<ver>.md`                                                                             |
| `migrations/types.ts` | `MigrationSpec`, `AppliesTo`, `VersionEntry`, `VersionsManifest` types                                                     |

`applyStructural` in `index.ts` searches each overlay (`shared/`, `enterprise/`, `standalone/`) when resolving a migration's `op.sourcePath`, so `add-file` operations work regardless of which overlay the template was authored in.

### The `merge/` module

| File                 | Responsibility                                                   |
| -------------------- | ---------------------------------------------------------------- |
| `json-deep-merge.ts` | Deep merges patch into JSON, respects `_managed` annotations     |
| `toml-dep-bump.ts`   | Bumps versions in `Cargo.toml` workspace deps and `package.json` |

---

## Migration Reference

Migrations live in the `migrations` array of a version entry in [packages/create-one-terminal/versions.json](packages/create-one-terminal/versions.json). Each migration has:

- `id` — unique string, used to skip already-applied migrations on re-runs
- `target` — relative path from the scaffolded workspace root (always without an overlay prefix — the wizard strips it during authoring)
- `appliesTo` _(optional)_ — `"both"`, `"standalone"`, or `"enterprise"`. The wizard infers this from which overlay the changed template lives in: `shared/` → `"both"`, `enterprise/` → `"enterprise"`, `standalone/` → `"standalone"`. Older migrations without this field default to `"both"` for backward compatibility with pre-variant scaffolds.

`buildChain` filters out migrations whose `appliesTo` does not match the workspace variant before any are applied. You generally don't write `appliesTo` by hand — let the wizard set it from the overlay context.

### `config-merge`

Deep-merges a `patch` object into a JSON config file. Only fields listed in the file's `_managed` array will be written; user-modified values outside that array are left untouched.

Use this for: adding a new key to `agent.config.json`, updating a URL default, adding a new section.

```json
{
  "type": "config-merge",
  "id": "0.2.0-add-dacpBridge-port",
  "target": "apps/desktop-agent/src-tauri/resources/agent.config.json",
  "description": "Add dacpBridge port to agent config",
  "appliesTo": "enterprise",
  "patch": {
    "ports": {
      "dacpBridge": 4475
    }
  }
}
```

To make a config field managed (so the upgrade tool may write to it), add its key to the `_managed` array in the source file:

```json
{
  "_managed": ["appDirectoryUrl", "engineCatalogUrl", "ports.dacpBridge"],
  "appDirectoryUrl": "http://localhost:3005"
}
```

### `dep-bump`

Bumps a dependency version in `Cargo.toml` (workspace dependencies) or `package.json`.

Use this for: updating `ot-fdc3`, `ot-core`, or any shared package version shipped with the framework.

```json
{
  "type": "dep-bump",
  "id": "0.2.0-bump-ot-fdc3",
  "target": "Cargo.toml",
  "description": "Bump ot-fdc3 to 0.2.0",
  "deps": [{ "name": "ot-fdc3", "ecosystem": "cargo", "newVersion": "0.2.0" }]
}
```

```json
{
  "type": "dep-bump",
  "id": "0.2.0-bump-fdc3-client",
  "target": "package.json",
  "description": "Bump @one-terminal/fdc3-client to 0.2.0",
  "deps": [{ "name": "@one-terminal/fdc3-client", "ecosystem": "npm", "newVersion": "0.2.0" }]
}
```

### `structural`

Applies line-level patch operations to any file. Use for changes that cannot be expressed as a JSON merge or dep bump — for example, adding a Tauri plugin registration line to `lib.rs`, or inserting a new npm script into `package.json`.

```json
{
  "type": "structural",
  "id": "0.2.0-register-new-plugin",
  "target": "apps/desktop-agent/src-tauri/src/lib.rs",
  "description": "Register the new ot-metrics plugin",
  "operations": [
    {
      "op": "insert-after-line-matching",
      "pattern": ".plugin(ot_fdc3::init())",
      "content": "        .plugin(ot_metrics::init())"
    }
  ]
}
```

**Available operations:**

| `op`                         | Behaviour                                                                         |
| ---------------------------- | --------------------------------------------------------------------------------- |
| `insert-after-line-matching` | Inserts `content` on the line immediately after the first line matching `pattern` |
| `replace-line-matching`      | Replaces the first line matching `pattern` with `replacement`                     |
| `add-file`                   | Copies `sourcePath` (relative to package root) to `targetPath` in the workspace   |

Structural migrations are applied in declaration order. Keep operations minimal and targeted — prefer `config-merge` or `dep-bump` when they cover the use case.

---

## Testing Changes Locally

Run these checks after any framework change before opening a PR.

### Test the framework itself

Verify the Rust workspace and scaffolder compile cleanly:

```sh
cargo check --workspace
npm run build:scaffolder
```

Then start all services to confirm everything connects end-to-end:

```sh
# Terminal 1 — App Directory (must start first)
npm run dev:app-directory       # http://localhost:3005

# Terminal 2 — Desktop Agent (FDC3 broker)
npm run dev:desktop-agent

# Terminal 3 — Terminal window manager
npm run dev:terminal

# Optional — sample apps to load inside the Terminal
npm run dev:sample-ticker          # http://localhost:3010
npm run dev:sample-chart           # http://localhost:3011
```

**Expected:** Desktop Agent connects to App Directory on startup. Terminal window opens and loads the app list. Sample apps appear as launchable entries.

---

### Test a scaffolded workspace

For a fast non-interactive check that both variants build, use the smoke test:

```sh
npm run build:scaffolder
npm run test:scaffold                       # both variants: cargo check + npm install + new-widget
npm run test:scaffold -- --variant standalone   # one variant only
npm run test:scaffold -- --keep             # don't delete the output (paths printed at end)
```

For an end-to-end manual check, rebuild templates and run the scaffolder interactively:

**Step 1 — Rebuild templates and scaffolder**

```sh
npm run extract-templates    # re-derive EJS templates from apps/ + overlay-overrides/
npm run build:scaffolder     # compile create-one-terminal to dist/
```

**Step 2 — Scaffold a test workspace**

Run the local scaffolder from the repo root:

```sh
node packages/create-one-terminal/dist/index.js
```

The prompts are:

1. **Workspace name** — kebab-case, e.g. `acme-trading`
2. **Output folder** — defaults to `./<workspace-name>`, accept or change it
3. **Workspace variant** — Standalone (default) or Enterprise
4. **Reverse-domain identifier** — e.g. `com.acme.trading`
5. **External FDC3 agent URL** _(Standalone only)_ — optional; leave blank to configure later
6. **Include FDC3?** _(Enterprise only)_ — defaults to yes
7. **Customize ports?** — defaults to no (Standalone asks only the Terminal dev port; Enterprise asks all six)

After confirming, move into the output folder:

```sh
cd <output-folder>
```

**Step 3 — Verify the Rust workspace compiles**

```sh
cargo check --workspace
```

Should exit 0. Warnings are fine; errors indicate a broken template.

**Step 4 — Install JS dependencies and build the App Directory**

```sh
npm install
npm run build:app-directory
```

**Step 5 — Run the scaffolded framework**

For an **Enterprise** workspace:

```sh
# Terminal 1
npm run dev:app-directory

# Terminal 2
npm run dev:desktop-agent

# Terminal 3
npm run dev:terminal
```

For a **Standalone** workspace:

```sh
# Terminal 1
npm run dev:sample-widget

# Terminal 2
npm run dev:terminal
```

**Passing signal:**

| Check                     | Expected                                                  |
| ------------------------- | --------------------------------------------------------- |
| `cargo check --workspace` | exits 0                                                   |
| `npm install`             | no peer dep errors                                        |
| Terminal starts           | window opens, launcher shows sample widget(s)             |
| App Directory starts      | responds at `http://localhost:<port>` _(Enterprise only)_ |
| Desktop Agent starts      | connects to App Directory, no crash _(Enterprise only)_   |

---

### Test the upgrade path

Run this after `npm run create-migration` has written a new version entry and you want to confirm the migrations apply correctly to an existing workspace.

**Step 1 — Scaffold a workspace on the previous version**

Before running the wizard, scaffold a test workspace so it starts on the old version. After the wizard runs and bumps the version, that test workspace is now "behind" and can be upgraded.

Alternatively, if you already have a scaffolded workspace from before your change, use that.

**Step 2 — Rebuild the scaffolder**

```sh
npm run build:scaffolder
```

**Step 3 — Run the upgrade command from inside the scaffolded workspace**

```sh
cd <your-scaffolded-workspace>
node /path/to/framework/packages/create-one-terminal/dist/index.js upgrade
```

**Step 4 — Check the upgrade report**

```sh
cat .one-terminal/upgrade-report-<ver>.md
```

All migrations should show status `applied`. A status of `needs-manual` means the migration detected a conflict and left a note for the user — verify the note is accurate and actionable.

**Step 5 — Verify the workspace still builds**

```sh
cargo check --workspace
npm install
```

The upgraded workspace should compile cleanly. Any build error points to a missing or incorrect migration operation.

---

## Publishing a Release

Publishing is fully automated once a `v*` tag is pushed. The CI publish workflow ([`.github/workflows/publish-scaffolder.yml`](.github/workflows/publish-scaffolder.yml)):

1. Re-extracts templates from source apps
2. Asserts `git diff --exit-code` on the templates directory — fails if templates are out of sync (prevents a stale publish)
3. Builds `create-one-terminal`
4. Runs `npm publish`

To cut a release:

```sh
# Ensure main is clean and all changes are merged
git checkout main
git pull

# Tag — semver, must start with v
git tag v0.2.0
git push origin v0.2.0
```

> The publish workflow requires `NPM_TOKEN` to be set as a repository secret with publish access to the `create-one-terminal` package.

---

## CI Workflows

### Template drift check

**File:** [`.github/workflows/template-drift.yml`](.github/workflows/template-drift.yml)

**Triggers on:** push or PR touching `apps/**`, `packages/ot-*/**`, or `Cargo.toml`

Runs `scripts/extract-templates.ts` into `/tmp`, then diffs the output against the committed templates. Fails with a clear diff if they diverge. The failure message tells the contributor exactly what to do:

> "Run `npm run extract-templates` locally and commit the updated templates."

This is the primary guard against templates drifting out of sync with the live apps. It should never need modification unless new source trees are added to the extractor's `SOURCES` array — in which case also add those paths to the `paths:` filter in this workflow.

### Publish scaffolder

**File:** [`.github/workflows/publish-scaffolder.yml`](.github/workflows/publish-scaffolder.yml)

**Triggers on:** push of any `v*` tag

Re-extracts templates, asserts no drift, builds, and publishes. The double-extraction (developer ran it locally before tagging, workflow runs it again) is intentional — it prevents a tag from being pushed on a branch where the developer forgot to re-extract.

---

## Naming Conventions

- Framework name: **OneTerminal** (not "One Terminal" or "one-terminal" in prose)
- Container window app: **Terminal** (the `one-terminal` Tauri app)
- Broker app: **Desktop Agent** (the `desktop-agent` Tauri app)
- Rust package names: `one-terminal`, `desktop-agent`, `tauri-webview-host`, `ot-core`, `ot-fdc3`
- npm package names: `@one-terminal/*`
- All environment variables: `OT_*` prefix

Preserve these in both source app code and in the substitution table in [scripts/extract-templates.ts](scripts/extract-templates.ts). If a name ever changes in the apps, update the substitution table simultaneously so the templates stay consistent.
