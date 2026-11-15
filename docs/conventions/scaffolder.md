# Scaffolder and template conventions

`create-one-terminal` turns the live framework source into EJS templates, so most changes
to `apps/` and `packages/ot-*` also change what new workspaces get, and may need a
migration for existing ones. Full mechanics: [CONTRIBUTING.md](../../CONTRIBUTING.md).

## What is extracted

`scripts/extract-templates.ts` walks these sources (`SOURCES`):

| Overlay      | Sources                                                                                                                                                                                                                             |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `shared`     | `apps/one-terminal`, `packages/ot-core`, `packages/fdc3-plugin` (built bundle only), `apps/sample-widget`, root `.gitignore`                                                                                                        |
| `enterprise` | `apps/desktop-agent`, `apps/tauri-webview-host`, `apps/app-directory`, `apps/app-directory-server`, `packages/ot-appdirectory`, `apps/sample-ticker`, `apps/sample-chart`, `apps/electron-host`, root `Cargo.toml` / `package.json` |
| `standalone` | Hand-authored files in `packages/create-one-terminal/overlay-overrides/standalone`                                                                                                                                                  |

`overlay-overrides/shared` adds hand-authored files to the `shared` overlay.

Skipped: `node_modules`, `target`, `dist`, `gen`, `.git`, `binaries`, AI context files
(`AGENTS.md`, `CLAUDE.md`), and the paths in `SKIP_PATHS`. Framework-only root scripts are
stripped via `ROOT_PACKAGE_SCRIPTS_OMIT`.

## Rules

1. **Never hand-edit** `templates/**`, `static-manifest.json`, `versions.json` or
   `scaffoldVersion` in `src/create/context.ts` (except the no-migration case in
   CONTRIBUTING). `overlay-overrides/` _is_ hand-edited.
2. **Product names are substitution keys.** `com.one-terminal.*`, `@one-terminal/*` and
   friends are replaced via `SUBSTITUTIONS`. Write those literals exactly; if you
   introduce a new user-customisable value, add a `ScaffoldContext` field, a prompt, and a
   substitution pair (most specific first) **before** extracting.
3. **New framework-only files** (dev scripts, internal docs, test harnesses) in an
   extracted tree must be added to `SKIP_PATHS` or `SKIP_FILENAMES`, or they will ship
   to every workspace.
4. **Every extracted change needs a decision:** migration (`config-merge`, `dep-bump` or
   `structural`) or "template refresh only". Record the choice in the PR description.
5. **Version bump:** patch for fixes, minor for additive features, major when an existing
   workspace's layout or config must change in a non-mergeable way (`breaking: true` in
   `versions.json`).
6. Templates must render for **both** variants (`enterprise`, `standalone`) — a `shared`
   file can't reference enterprise-only crates or packages unconditionally (use `ot:if`
   conditional lines; see CONTRIBUTING).

## Change workflow

```sh
# 1. edit apps/ or packages/ot-*
npm run check-template-drift      # shows which templates your change affects
npm run create-migration          # interactive (or --plan / --spec, below)
npm run build:scaffolder
npm run test:scaffold             # renders a workspace to a temp dir, cargo checks it
# 2. if a migration was added, test the upgrade path (CONTRIBUTING → "Test the upgrade path")
```

AI agents follow the [framework-upgrade](../../.ai/skills/framework-upgrade/SKILL.md)
workflow and the [scaffolder-steward](../../.ai/agents/scaffolder-steward.md) role, using
the non-interactive mode below.

## Non-interactive migrations

`create-migration` can take its answers from a JSON file instead of prompts:

```sh
npm run create-migration -- --plan target/migration-plan.json          # 1. detect
npm run create-migration -- --spec target/migration-plan.json          # 2. validate + dry run
npm run create-migration -- --spec target/migration-plan.json --yes    # 3. apply
```

`--plan` writes nothing but the plan: each changed template (`file`, `kind`, `target`,
`appliesTo`, `isNewFile`, auto-detected `patch` / `deps`, `defaultId`), `removedTemplates`,
`currentVersion`, `branchVersion`, `nextVersions`, and a `spec` skeleton. Fill in `spec`
and pass the same file to `--spec` (a bare spec object works too):

```jsonc
{
  "version": { "mode": "new", "bump": "minor", "breaking": false }, // or { "mode": "amend" }
  "files": {
    "shared/apps/one-terminal/package.json.ejs": {}, // accept auto-detected deps
    "shared/apps/one-terminal/src-tauri/tauri.conf.json.ejs": { "patch": { "newKey": true } },
    "shared/packages/ot-core/src/electron_host.rs.ejs": {
      "description": "Import Command for the new spawn helper",
      "operations": [
        {
          "op": "insert-after-line-matching",
          "pattern": "use std::path::{Path, PathBuf};",
          "content": "use std::process::Command;",
        },
      ],
    },
    "enterprise/apps/desktop-agent/src-tauri/src/lib.rs.ejs": {
      "skip": true,
      "note": "internal refactor",
    },
  },
}
```

Validation is strict, so a stale or mistyped spec fails instead of silently skipping:

- Every changed template needs an entry — a migration or `{ "skip": true }` — and entries
  for files that didn't change are rejected.
- Keys must fit the kind: `patch` (config-merge), `deps` (dep-bump), `operations`
  (structural), plus `id`, `description`, `skip`, `note`.
- Structural operations: `insert-after-line-matching` `{ pattern, content }`,
  `replace-line-matching` `{ pattern, replacement }`, `add-file` (new files only),
  `remove-path` `{ targetPath }`.
- When this branch already added a version, `version.mode` must say `"amend"` or `"new"`.

Write plan files under `target/` (gitignored). The non-interactive path doesn't shell out
to Unix tools, so it also works on Windows.

### Choosing what to migrate

- **Skip** only when existing workspaces don't need the change to keep building and
  working on the new version; new workspaces get it from the templates anyway.
- **Structural anchors** are matched against the user's file, which they may have
  edited — choose a unique, stable line. Changes too large to express as anchored edits
  belong in release notes as a manual step.
- **Removed JSON keys** are never migrated by `config-merge`; call them out.

### Known limitations

- **Deleted templates**: neither mode detects them for migration, and applying copies the
  new extraction over `templates/` without deleting anything. `--plan` lists them in
  `removedTemplates`; delete those files from `templates/` by hand (the one sanctioned
  hand edit) and add a `remove-path` operation if workspaces should lose the file.
- **`upgrade`** still prompts ("Apply migrations?" and JSON conflict resolution), so
  testing the upgrade path needs a person at a terminal.

## Review checklist

- [ ] `npm run check-template-drift` passes
- [ ] No hand edits under `templates/`
- [ ] New framework-only files are skipped by the extractor
- [ ] Migration present (or explicitly not needed) and version bumped appropriately
- [ ] `npm run test:scaffold` passes; upgrade path tested if a migration was added
- [ ] Both `enterprise` and `standalone` variants still render
