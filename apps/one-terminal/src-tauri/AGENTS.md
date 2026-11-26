# Terminal window manager (Rust) — agent context

Tauri 2 app using the **unstable multi-webview API** (`tauri` `features = ["unstable"]`):
one OS window per Terminal, the React chrome webview added first (lowest z-order), widget
panel webviews added on top with `add_child`. Rules:
[docs/conventions/rust.md](../../../docs/conventions/rust.md),
[docs/conventions/cross-platform.md](../../../docs/conventions/cross-platform.md).

## Module map

| Path                                 | Responsibility                                                                                                                      |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `src/lib.rs`                         | App setup, Tauri commands, `generate_handler!` list, run-event loop                                                                 |
| `src/layout/`                        | The `LayoutNode` tree (`node.rs`), mutations (`store.rs`), docking, drag, reflow, persistence, dashboards                           |
| `src/terminal/`                      | Multiple Terminal instances (`manager.rs`, `instances.rs`), spawn, popout windows + magnetic glue (`popout.rs`, `popout_snap.rs`)   |
| `src/agent.rs`                       | Which FDC3 Desktop Agent widgets connect to; injects `window.OT_FDC3_AGENT_URL`                                                     |
| `src/webview_pool.rs`                | Pre-warmed blank webviews (`OT_WEBVIEW_POOL_SIZE`)                                                                                  |
| `src/metrics.rs`                     | Terminal metrics: open windows/popouts, popout open/dock latency                                                                    |
| `src/engine.rs`, `engines.rs`        | Engine selection / spawning out-of-process hosts                                                                                    |
| `src/widgets.rs`, `sample_widget.rs` | Local widgets from `widgets.config.json`; embedded sample widget, served over loopback HTTP (never a custom scheme: those have IPC) |
| `src/widget_url.rs`                  | Widget URL policy (`ot_core::widget_url`, no exceptions); apply it at every webview-creation site                                   |

## Rules for changes here

- **Rust is the source of truth for layout.** Mutate the tree, then emit `wm:layout` and
  `wm:host-layout`; the frontend only renders. Keep the emit after every mutation path.
- **Every new `#[tauri::command]`** must be added to `generate_handler!` in `lib.rs`, and
  any core permission it needs added to `capabilities/default.json` — with the narrowest
  permission that works. See [security.md](../../../docs/conventions/security.md).
- **Window geometry differs per OS** — e.g. `terminal/popout.rs` uses `POPOUT_EDGE = 0` on
  macOS (frameless windows resize from edges over child views) and `4.0` elsewhere (the
  Windows resize hit-test needs bare window edge). Dock/reopen handling (`RunEvent::Reopen`)
  is macOS-only. Any geometry, focus, minimise or z-order change must be reasoned about on
  both OSes.
- `lib.rs` and `layout/store.rs` are large; prefer adding a focused module over growing
  them further.
- Moving a tab must reuse the same webview (that's how sessions survive re-docking) —
  never recreate a panel webview to move it.

## Test

```sh
cargo test -p one-terminal
cargo clippy -p one-terminal -- -D warnings
```

## Ships to scaffolded workspaces

Extracted into the scaffolder's `shared` overlay — see
[packages/create-one-terminal/AGENTS.md](../../../packages/create-one-terminal/AGENTS.md).
