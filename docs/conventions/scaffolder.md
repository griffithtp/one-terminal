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
npm run create-migration          # interactive: classify, author migrations, bump version
npm run build:scaffolder
npm run test:scaffold             # renders a workspace to a temp dir, cargo checks it
# 2. if a migration was added, test the upgrade path (CONTRIBUTING → "Test the upgrade path")
```

`create-migration` is interactive (`@clack/prompts`) and can't run unattended. An AI
agent should prepare the answers for each prompt (migration kind, target file,
anchor/insertion for `structural`, version bump) and hand them to a human — or run
`npm run extract-templates` when no migration is needed.

## Review checklist

- [ ] `npm run check-template-drift` passes
- [ ] No hand edits under `templates/`
- [ ] New framework-only files are skipped by the extractor
- [ ] Migration present (or explicitly not needed) and version bumped appropriately
- [ ] `npm run test:scaffold` passes; upgrade path tested if a migration was added
- [ ] Both `enterprise` and `standalone` variants still render
