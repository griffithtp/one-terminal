---
name: feature
description: Build a OneTerminal feature end to end with the agent pipeline — requirements plan, design, implementation issue by issue, tests, specialist reviews (Rust, React, cross-platform, FDC3, security, scaffolder) and handover — stopping for the person's approval at each gate. Use for any non-trivial feature or change request.
---

# Feature pipeline

Roles live in `.ai/agents/`. Where the assistant supports sub-agents (Claude Code
subagents, Copilot custom agents), run each role as one; otherwise follow the role file
yourself, one at a time. **Stop at every gate** and wait for the person — never skip a
gate because the next step looks obvious.

## 0. Set up

- Work on a feature branch, not `main` (`git switch -c feature/<name>`).
- Small, contained change (one file, an obvious fix)? Skip to step 3 and keep steps 4–6.

## 1. Requirements → gate

Run **requirements-analyst** with the person's request. It asks its questions in one
round, then writes `docs/plans/NN-<name>.md` and updates the plan indexes.

**Gate:** show the plan path, its summary and open questions. Proceed only when the
person approves the scope and has answered the open questions.

## 2. Design → gate

Run **architect** on the approved plan. Add its design to the plan under `### Design`,
and apply its "Plan fit" changes to the issues once the person agrees.

**Gate:** the person approves the design (or asks for changes — re-run the architect).

## 3. Implement, one issue at a time

For each issue in dependency order:

1. Read its Context/Objective/Acceptance criteria and the design.
2. Implement it, following `AGENTS.md` and the conventions docs.
3. Run **test-author** for the issue's tests (or write them yourself following that role).
4. Run the done checklist in `AGENTS.md`:

   ```sh
   cargo fmt --all && npm run format
   cargo clippy --workspace --all-targets -- -D warnings && npm run lint
   npm run typecheck
   cargo test -p <crate> ; npm run test:terminal ; npm run test:fdc3-plugin   # as touched
   ```

5. Tick the issue's acceptance criteria in the plan. Don't commit unless asked; if asked,
   one commit per issue.

## 4. Review

Run the reviewers that apply to the whole change — in parallel where possible:

| Run when the change touches                                    | Reviewer                                                |
| -------------------------------------------------------------- | ------------------------------------------------------- |
| `*.rs`, `Cargo.toml`                                           | rust-reviewer                                           |
| `apps/*/src`, `apps/app-directory/ui`                          | react-reviewer                                          |
| `src-tauri/`, `ot-core`, hosts, windows, keys, scripts, CI     | cross-platform-reviewer                                 |
| `broker/`, `fdc3_bus/`, `app_dir/`, `packages/fdc3-plugin/src` | fdc3-conformance                                        |
| capabilities, CSP, commands, schemes, bus, AppD, spawn, deps   | security-auditor                                        |
| `npm run check-template-drift` fails                           | scaffolder-steward → the **framework-upgrade** workflow |

Fix every **bug**/**blocker**/**critical**/**high** finding, and re-run the reviewer that
raised it on the fix. Lower-severity findings: fix, or list them for the person with a
reason. Don't argue findings away silently.

## 5. Verify on both OSes → gate

CI covers Linux, Windows and macOS builds and tests. Anything the reviewers marked "not
verifiable here" (window behaviour, popouts, focus, shortcuts, CSP in a production build)
needs a manual check. Write the exact steps per OS.

**Gate:** the person confirms the manual checks (or accepts them as follow-ups).

## 6. Hand over

Give the person:

- **Summary** — what changed and why, per issue.
- **Verification** — commands run with results; manual checks done or pending, per OS.
- **Review outcomes** — findings fixed, and findings deliberately left with reasons.
- **Scaffold impact** — migration decisions (from the framework-upgrade workflow).
- **PR description draft** — the above, condensed, linking the plan.
- **What to stage** — the file list; commit only if asked.
