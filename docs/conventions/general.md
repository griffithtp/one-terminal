# General conventions

Cross-cutting standards for all OneTerminal code: the must-follow rules, naming and
layout, the Rust ↔ TypeScript contract, errors and configuration, dependencies, comments,
Git, and language-level rules that apply everywhere. Area-specific rules live in the other
conventions docs — this file doesn't repeat them, it links to them.

| Area                   | Doc                                    |
| ---------------------- | -------------------------------------- |
| Rust                   | [rust.md](rust.md)                     |
| React / TypeScript     | [react.md](react.md)                   |
| Windows + macOS        | [cross-platform.md](cross-platform.md) |
| Security               | [security.md](security.md)             |
| FDC3                   | [fdc3.md](fdc3.md)                     |
| Scaffolder / templates | [scaffolder.md](scaffolder.md)         |

Rules marked **(new code)** describe where the codebase is heading; existing code that
doesn't follow them yet isn't a defect, but don't add more of it.

## Must-follow rules

A change that breaks one of these is wrong even if it works. Reviewers check every one.

1. **Rust owns Terminal layout state.** React renders `wm:layout` / `wm:host-layout` and
   calls commands; it never keeps its own copy of the layout. → [react.md](react.md#architecture-rules)
2. **The FDC3 broker is pure.** All FDC3 rules live in `broker/` with no sockets, axum or
   Tauri; I/O is an `Effect` performed by `fdc3_bus/`. → [fdc3.md](fdc3.md#where-code-goes)
3. **Shared crates stay Tauri-free.** `ot-core` and `ot-appdirectory` never depend on
   `tauri`. → [rust.md](rust.md#workspace)
4. **Widget content is untrusted.** No IPC for widget pages; capabilities grant only what
   app JS calls; CSP changes are deliberate. → [security.md](security.md)
5. **Both OSes, every time.** Every `cfg(target_os)` branch has a counterpart, paths use
   `PathBuf::join`, shortcuts use `CmdOrCtrl`. → [cross-platform.md](cross-platform.md)
6. **Generated files come from their generators.** Templates, `static-manifest.json`,
   `fdc3-plugin.js`, `CLAUDE.md` and `.claude/` copies are never hand-edited.
   → [AGENTS.md](../../AGENTS.md#generated-files--never-edit-by-hand)
7. **Framework changes reach workspaces deliberately.** A change to templated code gets a
   migration decision. → [scaffolder.md](scaffolder.md)
8. **Changed behaviour has a test.** Rust unit/broker tests, contract tests, Vitest/RTL.
   → [rust.md](rust.md#tests), [react.md](react.md#tests)
9. **The gates stay green.** Format, Clippy `-D warnings`, ESLint, typecheck, tests, drift
   checks, dependency audit, bundle budgets — fix the cause, never disable the check.

## Naming and layout

### Files

| Kind                | Convention                                                    | Example                                    |
| ------------------- | ------------------------------------------------------------- | ------------------------------------------ |
| React component     | `PascalCase.tsx`, styles in a sibling `PascalCase.css`        | `CommandPalette.tsx`, `CommandPalette.css` |
| Hook                | `useThing.ts` in `src/hooks/`                                 | `useLayout.ts`                             |
| Non-React TS module | `camelCase.ts` in `src/lib/` or `src/commands/`               | `keybindingStore.ts`                       |
| TS test             | `<name>.test.ts(x)` next to the code                          | `registry.test.ts`                         |
| Rust module         | `snake_case.rs`, or `snake_case/mod.rs` once it has children  | `webview_pool.rs`, `broker/mod.rs`         |
| Rust tests          | `#[cfg(test)] mod tests` in the file, or a sibling `tests.rs` | `broker/tests.rs`                          |

- **Exports:** named exports only. Default exports are reserved for entry points that a
  tool requires (`App.tsx`, Vite/Vitest configs).
- **One responsibility per module.** A new feature gets a new module; don't grow
  `one-terminal/src-tauri/src/lib.rs` or `layout/store.rs` further.
- **Shared TS types** that mirror Rust structs live in `src/types.ts`; types used by one
  module stay in that module.

### Names that cross a boundary

| Thing                | Convention                                | Examples                                                                 |
| -------------------- | ----------------------------------------- | ------------------------------------------------------------------------ |
| Tauri command        | `snake_case` with the app prefix          | `wm_open`, `wm_set_zoom` (Terminal); `cda_list_channels` (Desktop Agent) |
| Tauri event          | `<prefix>:<kebab-case>`                   | `wm:layout`, `wm:popout-snap`, `cda:connected`                           |
| Environment variable | `OT_SCREAMING_SNAKE`                      | `OT_FDC3_BUS_PORT`, `OT_WEBVIEW_POOL_SIZE`                               |
| Log tag              | `[component]` at the start of the message | `[wm]`, `[fdc3-bus]`, `[popout]`                                         |
| CSS class            | `wm-block__element--modifier`             | `wm-tab-ctx-menu__item--danger`                                          |
| npm package          | `@one-terminal/<name>`                    | `@one-terminal/fdc3-plugin`                                              |
| Keybinding           | Tauri accelerator with `CmdOrCtrl`        | `CmdOrCtrl+Shift+K`                                                      |

A few older commands (`close_tab`) predate the prefix; new commands always use it. These
names are substitution keys or public API for scaffolded workspaces — renaming one is a
breaking change (see [scaffolder.md](scaffolder.md)).

## The Rust ↔ TypeScript contract

Everything crossing IPC is a typed contract; both sides change in the same commit.

- **Field names:** Rust structs sent to JS use `#[serde(rename_all = "camelCase")]`; enum
  tags that are plain words may use `"lowercase"`. Match the TS side exactly.
- **Optional fields:** pair Rust and TS deliberately —

  | Rust                                                                 | TypeScript         |
  | -------------------------------------------------------------------- | ------------------ |
  | `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]` | `field?: T`        |
  | `Option<T>` without `skip_serializing_if` (serialises as `null`)     | `field: T \| null` |

  Mixing them up gives a TS type that claims `undefined` while JS receives `null`.

- **Payloads are named types.** Every command argument/return and event payload has a
  named Rust struct and a named TS type in `src/types.ts` — no inline object shapes.
- **Commands take one shape of arguments.** JS passes camelCase keys
  (`invoke("wm_set_zoom", { label, zoomFactor })`); Tauri maps them to the Rust snake_case
  parameters. Don't pass extra keys.
- **Errors:** commands return `Result<T, String>` with context in the message (see
  [rust.md](rust.md#errors)). **(new code)** When the UI must react differently to
  different failures, return a structured error instead —
  `#[derive(Serialize)] struct CmdError { code: &'static str, message: String }` with a
  matching TS type — so the UI branches on `code`, never on message text.

## Errors, logging and configuration

- **Fail loudly at the boundary, handle it at the caller.** Validate external input
  (files, env, JSON, IPC args, App Directory records) where it enters; inside, trust your
  own types.
- **Never swallow errors silently.** In Rust, `let _ =` only for best-effort operations;
  in TS, `.catch(console.error)` at minimum — a bare `.catch(() => {})` needs a comment.
- **Logging:** `eprintln!` / `println!` with a `[component]` tag; no secrets, tokens or
  full FDC3 context payloads. Timings follow the `[tag] <operation> in N ms` pattern
  used by popouts. (There is no log-level crate; adopting `tracing` would be a separate
  decision.)
- **New settings** go through the layered config model: a compiled-in default → a key in
  `agent.config.json` (or the Terminal's config) → an `OT_*` override if it's
  operationally useful → a row in the `AGENTS.md` env-var table → a test of the layering.
  Unknown keys warn and are ignored; never crash on config.
- **No magic numbers.** Name constants and say why the value is what it is
  (`APP_LAUNCH_TIMEOUT`, `POPOUT_EDGE`).

## Dependencies

- **Justify every new dependency.** Prefer the standard library or an existing
  dependency. Before adding one, check: maintenance and advisories (`cargo deny`,
  `npm audit`), license against `deny.toml`'s allow-list, and — for UI code — bundle cost
  (`npm run check:bundle-size`).
- **Right section.** Rust versions shared by more than one crate go in
  `[workspace.dependencies]` and members use `{ workspace = true }`. npm: runtime code in
  `dependencies`, build/test tooling in `devDependencies`.
- **Minimal features.** Enable only the crate features you use (e.g. `zip` with
  `deflate-flate2-zlib-rs` only).
- **Upgrades** are their own commit (`chore(deps): …`) with the lockfile, and note any
  scaffold impact (dependency versions in templated manifests need a `dep-bump`
  migration).

## Comments and documentation

- **Comment the why, not the what.** The code says what it does; comments explain
  constraints, platform quirks, ordering requirements and rejected alternatives.
- **Module headers:** every Rust module starts with a `//!` comment stating its
  responsibility; non-trivial TS modules start with a short block comment.
- **Public items:** `///` on public Rust items; JSDoc on exported TS functions and
  types whose purpose isn't obvious from the name.
- **No commented-out code** — delete it; Git has the history.
- **(new code)** `TODO` / `FIXME` name an owner or issue: `// TODO(#123): …`. An
  unowned TODO is a bug report nobody will read.
- **Docs change with the code.** A change to behaviour, config, env vars, commands or
  conventions updates the matching `AGENTS.md`, conventions doc or plan in the same PR.

## Git and pull requests

- **Branches:** work on `feature/<name>` or `fix/<name>`; never commit to `main` directly.
- **Commit messages:** [Conventional Commits](https://www.conventionalcommits.org/) with a
  scope, as used on `main`:

  ```
  <type>(<scope>): <imperative summary, lower case, no period>

  feat(one-terminal): widget pop-out windows with magnetic glue groups (#70)
  fix(fdc3): make widget channel selection reach the widget and Desktop Agent (#68)
  feat(fdc3)!: upgrade to FDC3 3.0 and consolidate on one client and protocol (#69)
  ```

  Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`.
  Scopes: `one-terminal`, `desktop-agent`, `fdc3`, `appd`, `ot-core`, `scaffolder`,
  `electron-host`, `deps`, `ci`, `ai`. `!` marks a breaking change; explain it in the body.

- **One logical change per PR.** Refactors go in their own PR before the feature that
  needs them.
- **PR description:** what and why; trust boundaries touched and how input is validated;
  scaffold/migration decisions; manual checks done on Windows and macOS (or pending);
  commands run with results. The `feature` workflow's handover produces this.

## Language rules

### Rust

In addition to [rust.md](rust.md):

- No `panic!`, `todo!`, `unimplemented!` or `unreachable!` outside tests unless the
  invariant is proven locally — and say why in a comment.
- `#[must_use]` on functions whose result is easy to ignore by mistake (builders,
  computed values, `Result`-like returns that aren't `Result`).
- Don't `.clone()` to quiet the borrow checker — restructure ownership. Clone at the
  boundary where ownership is genuinely needed.
- Parameters: `&str` / `&[T]` / `impl AsRef<Path>`; return owned types.
- Shared mutable state sits behind a type with methods (`TerminalManager`,
  `AgentSelection`), not raw `Arc<Mutex<…>>` passed through command handlers.
- Prefer `match` / `if let` / `let … else` over nested `unwrap_or` chains when the
  error path matters.
- Iterators over index loops; no `as` casts that can truncate without a comment
  (prefer `try_from`).

### TypeScript

In addition to [react.md](react.md):

- `strict` everywhere; no `any` in new code (use `unknown` and narrow); no non-null `!` —
  narrow or handle the `null`.
- `import type { … }` for type-only imports.
- String-literal unions, not `enum`: `type SplitDir = "horizontal" | "vertical"`.
- Exhaustive `switch` over unions with a `never` check in `default`:

  ```ts
  default: {
    const unreachable: never = group;
    throw new Error(`unhandled group ${unreachable}`);
  }
  ```

- `readonly` for arrays and objects a function doesn't own (props, IPC payloads).
- Every promise is awaited, returned or `.catch`-ed — no floating promises.
- `const` by default; no mutation of function arguments.

### React

In addition to [react.md](react.md):

- Function components only; props typed with an `interface Props` next to the component.
- State lives at the lowest component that needs it; there is no global store — Rust is
  the store, hooks subscribe to it.
- Derive values during render instead of copying props into state with effects.
- One `useEffect` per concern, each with its own cleanup; dependency arrays are honest
  (a disabled `exhaustive-deps` needs a comment explaining why).
- Memoise (`memo`, `useMemo`, `useCallback`) when a profile shows a cost or a child effect
  depends on identity — not by reflex.
- Event handlers are named `handleX` inside components and `onX` as props.
- Keys are stable ids (webview `label`, command `id`), never array indexes.

### Accessibility

Accessibility is a requirement, not polish:

- Interactive elements are `<button>`, `<a>` or form controls — never a clickable `<div>`.
- Menus use `role="menu"` / `menuitem` / `menuitemradio` / `menuitemcheckbox` with the
  matching `aria-checked` / `aria-expanded` state (as `WidgetMenuItems` does).
- Everything works from the keyboard: arrows, Enter, Escape; dialogs trap focus and return
  it to the trigger on close.
- Icon-only buttons have an `aria-label`; colour is never the only signal (FDC3 channel
  dots also carry a name).
- Tests query by role and accessible name, which keeps this honest.
