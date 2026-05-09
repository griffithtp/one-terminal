# High Impact Improvements

## 1. Add a Test Suite

**Status:** Not started  
**Scope:** All apps and packages

Zero TypeScript/JS tests exist across all 7 apps and 5 packages. Only 5 Rust unit tests exist in `apps/desktop-agent/src-tauri/src/engines/router.rs`. The upgrade/migration path in the scaffolder is particularly at risk — it has complex conflict resolution logic with no regression coverage.

**Actions:**

- Add **vitest** to `apps/app-directory`, `packages/fdc3-client`, and `packages/create-one-terminal`
- Write unit tests for all migration types in `packages/create-one-terminal/src/upgrade/` (dep-bump, config-merge, structural diff/anchor logic)
- Write unit tests for `packages/fdc3-client/src/agent.ts` and `bridge-agent.ts` using mocked Tauri IPC
- Expand Rust test coverage: add tests to `apps/desktop-agent/src-tauri/src/broker/`, `tcp/`, and `config.rs`
- Add `cargo test --workspace` to CI (see improvement #2)

**Why this matters:** The scaffolder upgrade path is the most likely thing to silently regress. It's pure TypeScript and fully testable without a Tauri environment.

---

## 2. Add a CI Build-Check Workflow

**Status:** Done — `.github/workflows/ci.yml` added (two parallel jobs: `rust` and `typescript`)  
**Scope:** `.github/workflows/`

Current CI only enforces template drift and publishes the scaffolder on tags. There is no workflow that runs `cargo check --workspace` or `npm run build:app-directory` on pull requests. A broken build won't surface until someone runs it locally.

**Actions:**

- Add `.github/workflows/ci.yml` that triggers on all PRs and pushes to `main`
- Steps: `cargo check --workspace`, `cargo test --workspace`, `npm ci`, `npm run build:app-directory`
- Once tests exist (see improvement #1), add `npx vitest run` to the same workflow
- Consider adding a matrix for macOS + Windows to catch platform-specific Rust issues

---

## 3. Enforce Linting and Formatting

**Status:** Done — `rustfmt.toml`, `eslint.config.js`, `.prettierrc`, `.prettierignore` added; lint/format/typecheck scripts wired to all packages; formatting applied to entire codebase; enforced in CI  
**Scope:** Workspace root

No `rustfmt`, `eslint`, or `prettier` config is enforced anywhere. Because the framework ships as a scaffolded template, style inconsistencies in source files propagate into every downstream workspace.

**Actions:**

- Add `rustfmt.toml` at workspace root; run `cargo fmt --check` in CI
- Add `eslint.config.js` and `.prettierrc` for TypeScript packages (`fdc3-client`, `fdc3-plugin`, `app-directory`, `create-one-terminal`)
- Add lint/format check steps to the CI workflow from improvement #2
- Consider adding a pre-commit hook via `lefthook` or `husky` for local enforcement
