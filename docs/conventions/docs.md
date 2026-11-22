# Documentation conventions

Who each doc is for, where each fact lives, what has to be updated when code changes, and
how docs reach the repo. `npm run check:docs` enforces the mechanical parts (env vars,
links, index tables, plan references); the `docs-steward` agent handles the rest.

## Audiences

Write for the reader of that doc, not for whoever is reading the diff.

| Doc                                                                   | Reader                                         | How to write it                                                                                                                                  |
| --------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Root and nested `AGENTS.md`                                           | AI agents, every session                       | Terse facts, tables and paths. No tutorials or history: every line costs context in every session. Add rows, not paragraphs                      |
| `docs/conventions/*.md`                                               | Framework contributors (human and AI)          | Rules with the reason and a link to the code. Mark direction-of-travel rules **(new code)**                                                      |
| `CONTRIBUTING.md`, `SCAFFOLDING.md`                                   | Framework contributors                         | Setup and workflows, step by step                                                                                                                |
| App and package `README.md`                                           | Widget developers, scaffolded-workspace users  | What it does, how to run and configure it, with working examples. Five of these ship into workspaces (see [Template READMEs](#template-readmes)) |
| Root `README.md`                                                      | People evaluating or starting with OneTerminal | Overview, quick start, features. Workspaces get their own README instead                                                                         |
| `docs/fdc3.md`, `docs/telemetry.md`, `docs/signing.md`, release notes | Widget developers, release managers, end users | Task-focused guides                                                                                                                              |
| Plans and design notes (`.work/`, never committed)                    | Approvers and implementers                     | See [AGENTS.md → Plans](../../AGENTS.md#plans). Committed files never link to or name them                                                       |

## Where a fact lives

One home per fact; other docs link to it instead of copying it.

| Fact                               | Home                                                                                                                                                                               |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OT_*` env vars                    | Root `AGENTS.md` → Environment variables (public) or Internal variables (set by OneTerminal for its own child processes). App-specific detail and examples go in that app's README |
| Ports                              | Root `AGENTS.md` → Key port defaults                                                                                                                                               |
| npm and cargo commands             | Root `AGENTS.md` → Build / check commands                                                                                                                                          |
| Apps and packages                  | Root `AGENTS.md` → Workspace layout; module maps in the nested `AGENTS.md`                                                                                                         |
| Agents and workflows               | Root `AGENTS.md` tables and `.ai/README.md`                                                                                                                                        |
| Naming (OneTerminal, Terminal, …)  | Root `AGENTS.md` → Naming rules. Don't copy the list                                                                                                                               |
| Rules                              | The conventions doc for the area                                                                                                                                                   |
| Tauri commands and events          | Not listed one by one. The nested `AGENTS.md` documents the patterns (naming, registration, capabilities); the app README documents commands that are part of a public API         |
| Design decisions and their reasons | The code comment or conventions doc they affect. Plans are never committed, so a decision that matters later has to be written down in the repo itself                             |

## When docs must change

`general.md`'s rule "Docs change with the code" means a change to any of these updates the
docs in the same PR:

- a new, renamed or removed `OT_*` variable, or a changed default;
- a port;
- an npm script or cargo command that contributors or agents run;
- an app or package directory;
- a Tauri command or event, **only** when it changes a documented pattern or a public API;
- an App Directory record field or endpoint;
- an agent, skill or workflow in `.ai/`;
- any user-visible behaviour a README describes.

## How docs reach the repo

- **Docs go in the same PR as the code**, as its final commit: `docs(<scope>): …`. The
  feature workflow's step **5b — Docs** writes it after the code is verified (reviews
  fixed, CI build and test green, manual checks confirmed), so docs describe the code that
  ships and are written once.
- **Existing drift goes in its own PR** on a `docs/<name>` branch, so feature PRs carry no
  unrelated edits.
- The CI `docs` job runs `npm run check:docs`. It is separate from build and test, and
  required for merge.

## Code comments

- Describe the behaviour or the decision and its reason. Never cite a plan, epic or issue
  number from the planning folder: it isn't committed, so the reference is dead for every
  other reader.
- Links in doc comments (`//!`, `///`, `/** … */`) are checked like Markdown links.

## Links

- Relative links only for files in the repo, with **exact case**. macOS resolves
  `docs/Conventions/rust.md`; Linux CI and GitHub don't.
- Link to a heading with GitHub's slug (`#build--check-commands`).
- No links into `.work/` or outside the repo root.
- External links are fine but aren't checked; prefer stable URLs (specs, releases).

## Template READMEs

These READMEs are extracted into scaffolded workspaces, so editing one causes template
drift and needs the [framework-upgrade](../../.ai/skills/framework-upgrade/SKILL.md)
workflow (a template refresh; no migration for text-only changes):

- `apps/one-terminal/README.md`
- `apps/desktop-agent/README.md`
- `apps/app-directory/README.md`
- `apps/electron-host/README.md`
- `packages/fdc3-plugin/README.md`

The workspace root README is hand-written in
`packages/create-one-terminal/overlay-overrides/shared/README.md.ejs`. The framework's root
`README.md` doesn't ship.

## Working notes

Plans, design notes and scratch documents go in `.work/` (gitignored), never in `docs/`.
`docs/` holds only documentation of what the repo does today.
