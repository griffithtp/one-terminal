# Security conventions

OneTerminal runs third-party web content (widgets) next to privileged native code. The
core rule: **widget content is untrusted**. It must never reach Tauri IPC, the file
system or process spawning except through an explicitly validated path.

## Trust boundaries

| Boundary                                                         | Untrusted side                     | Guard                                                                                                                                                                 |
| ---------------------------------------------------------------- | ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Panel webview → Tauri IPC                                        | Widget pages (any URL)             | Tauri capabilities (local origins only)                                                                                                                               |
| Widget → FDC3 bus (`ws://127.0.0.1:7891/fdc3`)                   | Any local process or web page      | WCP handshake (see gaps below)                                                                                                                                        |
| Client → App Directory REST (`:3005`)                            | Admin UI, scripts, browsers        | Loopback bind; CORS on the standalone server; auth planned                                                                                                            |
| App Directory / plugin manifest → process spawn                  | Directory records, `manifest.json` | Engine catalog checksums; launch routing in `engines/router.rs`                                                                                                       |
| Config files / env vars                                          | Local user                         | Validation in `AgentConfig::load()`; unknown keys ignored                                                                                                             |
| UI webview → log relay (`wm_telemetry_log`, `cda_telemetry_log`) | Any webview that reaches IPC       | Webview-label allowlist (chrome / overlay / dashboard only); sender identity from the label, never the payload; `ot_telemetry::relay` caps, rate limit and sanitising |
| Parent → child process environment                               | Third-party plugin binaries        | `ot_telemetry::propagation::ChildKind`: OTLP credentials only to the Terminal (`Exporting`); stripped for hosts and plugins                                           |

## Tauri capabilities

- Capabilities live in each app's `src-tauri/capabilities/default.json`. Grant the
  **narrowest** permission that works (`core:window:allow-set-size`, not `core:window:default`
  plus extras), scoped to the narrowest window label set.
- Never add widget origins to a capability's `remote.urls`. Panel webviews must not have IPC.
  If a widget needs a native feature, expose it through FDC3 or a reviewed, purpose-built
  channel — not by widening capabilities.
- Every new `#[tauri::command]` validates its arguments as if they came from an attacker:
  labels and ids must refer to existing objects, paths must be canonicalised and kept
  inside an allowed root, URLs must be parsed and their scheme checked
  (`http`/`https` only for widgets).
- Grant only what the app's **JS** calls. Windows, webviews and events driven from Rust
  need no capability. Current grants:

  | App                  | Windows                                                       | Permissions beyond `core:default`                                                                                           |
  | -------------------- | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
  | `one-terminal`       | `terminal-main-chrome`, `terminal-main-overlay`, `terminal-*` | window `start-dragging`, `start-resize-dragging`, `minimize`, `toggle-maximize`, `close`; event `emit`, `emit-to`, `listen` |
  | `desktop-agent`      | `desktop-agent-dashboard`                                     | none                                                                                                                        |
  | `tauri-webview-host` | `*`                                                           | none (its only local page is a script-free stub)                                                                            |

  Adding a JS call to a new window/webview API means adding exactly that permission.

- Capability files ship to scaffolded workspaces as static files: new workspaces get the
  current version, existing ones keep theirs (no migration).

## Content Security Policy

Each `tauri.conf.json` sets `app.security.csp`. Tauri applies it to the app's own pages
(Terminal chrome, overlay and popout chrome; Desktop Agent dashboard; the host's stub
page) — **not** to widget pages, which load remote URLs or custom schemes.

| Directive                                            | Terminal / Desktop Agent    | Why                                     |
| ---------------------------------------------------- | --------------------------- | --------------------------------------- |
| `default-src`, `script-src`, `style-src`             | `'self'`                    | Vite emits only bundled JS/CSS files    |
| `img-src`, `font-src`                                | `'self' data:`              | Vite may inline small assets as `data:` |
| `connect-src`                                        | `ipc: http://ipc.localhost` | Tauri IPC (macOS / Windows); no `fetch` |
| `object-src`, `base-uri`, `form-action`, `frame-src` | `'none'`                    | Not used                                |

- React `style={{…}}` props are fine (set through the CSSOM). Literal `style="…"`
  markup, `<style>` elements, `dangerouslySetInnerHTML`, `innerHTML`, `eval` and remote
  scripts are not — and would also be blocked.
- Loading something new (a font, an image host, a `fetch` to the App Directory) means
  adding that exact source to the CSP in the same change.
- `devCsp` is `null`: the Vite dev server needs inline scripts and an HMR websocket. The
  CSP is therefore only exercised by **production builds** (`npm run build:terminal`,
  `npm run build:desktop-agent`) — test UI changes there before release.

## FDC3 bus

The bus binds to `127.0.0.1` only — keep it that way (never `0.0.0.0`).

**Known gaps (not yet fixed — don't build on these assumptions):**

- The `appId` is self-asserted: it comes from the `?appId=` query string (default
  `unknown-app`) and isn't checked against where the page was loaded from.
- The WebSocket upgrade doesn't check `Origin`. Browsers don't apply CORS to WebSockets,
  so any web page open in the user's normal browser can connect to the local bus.

Consequences: any page can claim to be any app, broadcast on user channels, raise intents
and trigger `open` (which launches apps). New broker features that grant power to a
specific `appId` (permissions, privileged intents) must first close these gaps — e.g.
an Origin allow-list, plus a per-panel token injected by the Terminal into each webview
it hosts.

## App Directory

- The embedded App Directory binds to `127.0.0.1` and has no CORS layer, so browsers block
  cross-origin JSON writes (preflight fails). The standalone server allows only the
  origins in `CORS_ORIGIN`.
- `POST /v2/apps`, `PUT` and `DELETE /v2/apps/:app_id` are **unauthenticated** — any local process
  can register an app, including its URL and engine binding. Auth is planned;
  until then, treat directory
  records as untrusted input when launching.

## Process spawning

- Engine runtime downloads are verified with SHA-256 against the catalog. Never add a
  download path that skips verification.
- Archives are extracted entry by entry (`extract_zip` in both apps): paths confined with
  `enclosed_name()`, contents streamed with `io::copy` (never sized from the entry
  header), no symlinks created, no permission bits applied. Don't switch to
  `ZipArchive::extract()`.
- Plugin `LaunchMode::SpawnBinary { binary_name, … }` is passed to `Command::new`, which
  does a `PATH` lookup. Resolve plugin binaries to an absolute path inside the plugin's own
  directory, and reject names containing path separators or `..`.
- Pass untrusted values (`{{url}}`, `{{title}}`, `{{app_id}}`) as discrete arguments or
  env vars — never through a shell (`sh -c`, `cmd /C`).
- Native app launch (planned) must be allow-list based
  (`OT_NATIVE_APP_ALLOW_PATHS`).

## Secrets and dependencies

- Never commit secrets. Signing keys live in GitHub Actions secrets ([docs/signing.md](../signing.md)).
- `OT_APPD_TOKEN` and future auth tokens must never be logged or written into scaffolded
  files.
- New dependencies: prefer well-maintained crates and packages; check the licence fits the
  repo's [LICENSE](../../LICENSE).
- `.github/workflows/audit.yml` runs on every change and weekly:
  `cargo deny check advisories bans licenses sources` (policy in [deny.toml](../../deny.toml))
  and `npm audit --omit=dev --audit-level=high`. A new advisory fails it even on unrelated
  PRs — upgrade the dependency, or add a `deny.toml` `ignore` entry whose reason shows the
  advisory doesn't apply. Never disable the job.
- Workspace crates are `publish = false`; that's what lets the license check skip them.

## Open questions

Not yet verified — check before relying on either:

- **Custom scheme widgets and IPC — answered: yes.** Tauri treats registered custom
  schemes as local origins (`is_local_url` in `tauri/src/webview/mod.rs`), so the
  `sample-widget://` page can call every Terminal app command. Today it serves only fixed,
  first-party files (`sample_widget.rs`), so this isn't exploitable, but any local widget
  served this way must be trusted code. The log relay refuses it by webview label. To
  close the gap: serve local widgets over `http://127.0.0.1`, or restrict app commands
  with an app manifest.
- **Electron host navigation.** `apps/electron-host` runs pages with
  `nodeIntegration: false` and `contextIsolation: true` and no preload, but doesn't
  restrict `window.open` or navigation. Low risk without Node access; decide whether
  hosted apps should be pinned to their origin.

## Reporting

When a change touches any boundary above, say so in the PR description and state how the
input is validated, and run the [security-auditor](../../.ai/agents/security-auditor.md).
