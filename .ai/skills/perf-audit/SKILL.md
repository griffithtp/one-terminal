---
name: perf-audit
description: Measure a OneTerminal branch's performance against main — first-paint bundle sizes for every UI against their budgets and the main baseline, plus runtime timings (popout open/dock, startup) where the change touches them — and report the deltas. Use before merging UI or hot-path changes, or when the bundle budget check fails.
---

# Performance audit

Measures this branch against `main` and reports what changed. Interpretation and
deeper investigation: the **performance-analyst** role (`.ai/agents/performance-analyst.md`).

## 1. Bundle sizes on this branch

```sh
npm -w @one-terminal/one-terminal run build
npm -w @one-terminal/desktop-agent exec -- vite build
npm run build:app-directory
npm run check:bundle-size -- --json target/bundle-branch.json
```

Any budget failure is a finding — keep going to see the baseline.

## 2. Baseline from main

Build `main` in a throwaway worktree under `target/` (gitignored):

```sh
git worktree add target/perf-main main
cd target/perf-main
npm ci --ignore-scripts
npm -w @one-terminal/one-terminal run build
npm -w @one-terminal/desktop-agent exec -- vite build
npm run build:app-directory
npx tsx scripts/check-bundle-size.ts --json ../bundle-main.json
cd ../..
npm run check:bundle-size -- --compare target/bundle-main.json
git worktree remove target/perf-main
```

If `main` predates `scripts/check-bundle-size.ts`, run this branch's script against the
worktree instead: copy `scripts/check-bundle-size.ts` and `scripts/bundle-budgets.json`
into `target/perf-main/scripts/` before measuring.

## 3. Runtime timings (when the change touches them)

| Change touches                    | Measure                                                                                                   |
| --------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Popouts (`terminal/popout.rs`)    | `[popout] open … in N ms` / `dock … in N ms` log lines, 10+ runs each, warm and cold — vs Plan 17 targets |
| Startup, restore, webview pool    | Time from launch to first paint and to restored layout (add an `[perf]` `Instant` log temporarily)        |
| Layout / drag / chrome components | Renders per gesture in the webview devtools Profiler (dev build)                                          |
| Broker / FDC3 bus                 | `Broker::handle` time over a message mix in a `--release` ignored test                                    |

Use release builds for reported numbers, the same machine for both sides, and medians.
Say which OS you measured on; the other OS is a manual follow-up.

## 4. Report

| UI / operation | Metric | main | branch | Δ   | Budget / target |
| -------------- | ------ | ---- | ------ | --- | --------------- |

Then: regressions with their cause (which chunk or import grew, which step got slower),
improvements worth locking in by **lowering** a budget in `scripts/bundle-budgets.json`,
and any budget raise needed — with the reason, for the PR description.
