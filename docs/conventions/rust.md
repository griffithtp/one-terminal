# Rust conventions

Applies to every crate in the Cargo workspace. Cross-platform rules are in
[cross-platform.md](cross-platform.md); security rules in [security.md](security.md).

## Tooling (enforced)

| Check  | Command                                                             | Where enforced        |
| ------ | ------------------------------------------------------------------- | --------------------- |
| Format | `cargo fmt --all` (`rustfmt.toml`: edition 2021, `max_width = 100`) | lefthook pre-push, CI |
| Lint   | `cargo clippy --workspace --all-targets -- -D warnings`             | lefthook pre-push, CI |
| Tests  | `cargo test --workspace`                                            | CI                    |

Lints are configured once in the root `Cargo.toml` (`[workspace.lints]`) and inherited by
every member with `[lints] workspace = true` — a new crate must add that too. Clippy runs
on `--all-targets`, so tests, examples and build scripts are linted as well.

| Lint                                                                                                              | Level           | Why                                                                                  |
| ----------------------------------------------------------------------------------------------------------------- | --------------- | ------------------------------------------------------------------------------------ |
| `unsafe_code`                                                                                                     | **forbid**      | There is no `unsafe` in the workspace; keep it that way                              |
| `dbg_macro`, `todo`, `unimplemented`, `undocumented_unsafe_blocks`                                                | warn (CI: deny) | No debugging leftovers or placeholders                                               |
| `clone_on_ref_ptr`, `implicit_clone`, `manual_let_else`, `semicolon_if_nothing_returned`, `uninlined_format_args` | warn (CI: deny) | Clarity: `Arc::clone(&x)` makes shared ownership visible, let-else for early returns |

Deliberately **not** enabled (measured 2026-11-15):

| Lint                       | Hits | Why it stays off                                                                                   |
| -------------------------- | ---- | -------------------------------------------------------------------------------------------------- |
| `unwrap_used`              | 362  | Includes the accepted `Mutex::lock().unwrap()` idiom; the rule below covers external input instead |
| `needless_pass_by_value`   | 184  | Tauri commands must take `State<…>`, `AppHandle` and `Window` by value                             |
| `expect_used`              | 21   | Build scripts, startup and stated invariants (`"about:blank is a valid URL"`)                      |
| `cast_possible_truncation` | 11   | Pixel geometry (`f64 → i32/u32`, saturating) and millisecond timestamps                            |
| `redundant_clone`          | 11   | A nursery lint with known false positives                                                          |
| `panic`                    | 7    | Almost all in tests                                                                                |

Never silence a Clippy lint crate-wide to get green. A targeted `#[allow(clippy::…)]` on
one item needs a comment saying why. Turning one of the "not enabled" lints on is welcome
once its count is driven down — update this table when you do.

## Workspace

- Shared dependency versions live in `[workspace.dependencies]` in the root `Cargo.toml`;
  members reference them with `{ workspace = true }`. Add a new shared dependency there
  first.
- `ot-core` and `ot-appdirectory` must stay free of Tauri. The Desktop Agent's `broker/`
  must stay free of Tauri, axum and tokio networking (it may hold an `UnboundedSender`).
- There is no `unsafe` in the workspace. Keep it that way; if FFI is ever unavoidable,
  isolate it in one module with a `// SAFETY:` comment on every block.

## Errors

- Tauri commands return `Result<T, String>` (the existing convention across both apps) —
  map errors with context: `.map_err(|e| format!("open dashboard {id}: {e}"))`.
- No `unwrap()` / `expect()` on values that come from outside the process: files, env
  vars, JSON, IPC arguments, network, the App Directory. Return an error or fall back to a
  logged default.
- `Mutex::lock().unwrap()` is the accepted idiom for in-process state (a poisoned lock
  means a thread already panicked). Don't hold a lock across `.await` or across a call
  that can re-enter the same lock (Tauri event handlers, `emit`).
- Best-effort window operations (`unminimize`, `set_focus`, …) use `let _ = …;`
  deliberately — only where failure genuinely doesn't matter.

## Logging

Log with the `tracing` macros; `println!`/`eprintln!` are only for `build.rs` directives
and stdout protocols (the `fdc3_bus_harness` example). Each binary calls
`ot_telemetry::init` once at startup — after loading its config, before anything else
logs — and `Telemetry::shutdown()` on `RunEvent::Exit` and before any
`std::process::exit`. Libraries (`ot-core`, the broker) depend on `tracing` only.

- **Levels:** `error!` — an operation failed and the user loses something (a launch, a
  save); `warn!` — degraded or ignored input (bad config value, unreachable App
  Directory); `info!` — lifecycle and one-off events (startup, connect/disconnect,
  restores, timings); `debug!` — per-operation detail useful when diagnosing.
- **No tag prefix:** the target (module path) identifies the component. Put identifiers
  in fields, not only in the message: `tracing::info!(app_id = %id, "connected")`.
- **Never log** secrets, tokens, header values, or full FDC3 context payloads (log the
  context `type`). Pass URLs through `ot_telemetry::redact::url`, which drops
  credentials and query strings.
- **Timings** stay `"<operation> in N ms"` in the message until a metric replaces them.
- **Spans** for work that crosses a boundary (an FDC3 request, a launch): create them
  with `tracing::info_span!` and use `.instrument(span)` on spawned futures so the span
  survives the `await`.

## Async and blocking

- Tokio is the runtime. Don't call blocking I/O (`std::fs` on large files,
  `std::process::Command::output`, `std::thread::sleep`) on an async task's thread — use
  `tokio::fs`, `tokio::process`, or `spawn_blocking`. Shutdown paths are the only exception.
- The broker is synchronous by design; timeouts are expressed as `Effect::ExpireAfter`
  and scheduled by the bus.

## Ownership

- `ot_core::engine::EngineFamily` is not `Copy` (it has a `Custom(String)` variant) —
  `.clone()` it when building structs from `&EngineBinding`.
- Prefer borrowing (`&str`, `&[T]`) in function parameters; clone at the boundary where
  ownership is actually needed, not defensively.
- On hot paths (FDC3 frame handling, layout emits) avoid needless `serde_json::Value`
  clones and re-serialisation.

## Style

- Every module starts with a `//!` doc comment explaining its responsibility and the
  non-obvious _why_ (see `broker/mod.rs`, `terminal/popout.rs`).
- Section separators use the existing `// ── Title ──────` style.
- Name constants for magic numbers (`APP_LAUNCH_TIMEOUT`, `POPOUT_EDGE`) and document the
  reason for the value.
- Large files (`one-terminal/src/lib.rs`, `layout/store.rs`) should shrink over time:
  put new functionality in a new focused module.

## Tests

- Unit tests sit in the same file under `#[cfg(test)] mod tests`, or in a sibling
  `tests.rs` for large modules (`broker/tests.rs`).
- Every broker rule, config layer and layout-tree mutation gets a test. Pure logic should
  be written so it can be tested without a Tauri `AppHandle`.
- Tests touching paths or env vars must not depend on the developer's real home dir —
  set the override env var (e.g. `OT_ENGINE_CACHE_ROOT`) or use a temp dir.
