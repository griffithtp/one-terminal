# `create-one-terminal` (scaffolder) — agent context

Generates new OneTerminal workspaces from EJS templates derived from the live apps, and
upgrades existing workspaces through versioned migrations. Full reference:
[CONTRIBUTING.md](../../CONTRIBUTING.md) ("Making Framework Changes", "The Template
System", "Migration Reference"). Rules: [docs/conventions/scaffolder.md](../../docs/conventions/scaffolder.md).

## Never hand-edit

- `templates/**` and `static-manifest.json` — produced by `scripts/extract-templates.ts`
- `versions.json` and `scaffoldVersion` in `src/create/context.ts` — written by the
  `create-migration` wizard (hand-edit only in the no-migration case described in CONTRIBUTING)
- `dist/` — build output

## Layout

| Path                      | Contents                                                                                        |
| ------------------------- | ----------------------------------------------------------------------------------------------- |
| `src/create/`             | `npm create one-terminal`: prompts, `ScaffoldContext` (`context.ts`), render                    |
| `src/upgrade/`            | `upgrade`: detect version, chain migrations, backup/restore, conflicts, report                  |
| `src/upgrade/migrations/` | Per-version migration specs (`v<semver>.ts`) of kind `config-merge`, `dep-bump` or `structural` |
| `src/merge/`              | JSON deep merge, TOML dep bump, managed-file handling                                           |
| `src/new-widget/`         | `create-widget` wizard                                                                          |
| `overlay-overrides/`      | Hand-authored variant files with no live-source equivalent (these **are** hand-edited)          |
| `widget-template/`        | New-widget template                                                                             |

Variants: `shared` overlay (all), `enterprise` (full stack), `standalone` (slim).

## Commands

```sh
npm run create-migration         # interactive wizard: extract + diff + author migrations + bump version
npm run extract-templates        # re-derive EJS templates only (no migration prompts)
npm run build:scaffolder         # compile to packages/create-one-terminal/dist/
npm run check-template-drift     # CI drift check — exits 1 if templates are out of sync
npm run test:scaffold            # render a workspace to a temp dir + cargo check it
node packages/create-one-terminal/dist/index.js            # scaffold a new workspace locally
node packages/create-one-terminal/dist/index.js upgrade    # run upgrade against current directory
```

`create-migration` is interactive by default. Agents and CI answer it from a file instead:

```sh
npm run create-migration -- --plan target/migration-plan.json          # detect; write plan + spec skeleton
npm run create-migration -- --spec target/migration-plan.json          # validate + dry run
npm run create-migration -- --spec target/migration-plan.json --yes    # apply
```

The spec format and decision rules are in
[docs/conventions/scaffolder.md](../../docs/conventions/scaffolder.md#non-interactive-migrations);
the end-to-end steps are the [framework-upgrade](../../.ai/skills/framework-upgrade/SKILL.md)
workflow. `upgrade` itself is still interactive (confirmation and conflict prompts).

## Upgrading the framework (developer workflow)

1. Edit the live app in `apps/` or `packages/ot-*`
2. Run `npm run create-migration` — the wizard extracts templates, diffs against committed ones, prompts for migration specs per changed file, bumps the version, updates `context.ts`, and copies the new templates
3. Review: `git diff packages/create-one-terminal/`
4. Run `npm run build:scaffolder` and test with a scaffolded workspace

## Testing a scaffolded workspace locally

```sh
npm run build:scaffolder
node packages/create-one-terminal/dist/index.js   # answer prompts, note output dir
cd <output-dir>
cargo check --workspace          # must exit 0
npm install
npm run build:app-directory
```

Then start the services in order: `dev:desktop-agent` (embeds the App Directory on 3005) → `dev:terminal`.

To test the upgrade path, scaffold a workspace _before_ running the wizard (so it starts on the old version), then after the wizard runs: `node /path/to/framework/packages/create-one-terminal/dist/index.js upgrade` from inside that workspace.
