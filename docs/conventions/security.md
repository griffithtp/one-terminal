# Security conventions

OneTerminal runs third-party web content (widgets) next to privileged native code. The
core rule: **widget content is untrusted**. It must never reach Tauri IPC, the file
system or process spawning except through an explicitly validated path.

## Trust boundaries

| Boundary                                        | Untrusted side                     | Guard                                                                |
| ----------------------------------------------- | ---------------------------------- | -------------------------------------------------------------------- |
| Panel webview → Tauri IPC                       | Widget pages (any URL)             | Tauri capabilities (local origins only)                              |
| Widget → FDC3 bus (`ws://127.0.0.1:7891/fdc3`)  | Any local process or web page      | WCP handshake (see gaps below)                                       |
| Client → App Directory REST (`:3005`)           | Admin UI, scripts, browsers        | Loopback bind; CORS on the standalone server; auth planned (Plan 03) |
| App Directory / plugin manifest → process spawn | Directory records, `manifest.json` | Engine catalog checksums; launch routing in `engines/router.rs`      |
| Config files / env vars                         | Local user                         | Validation in `AgentConfig::load()`; unknown keys ignored            |

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
- `tauri-webview-host`'s capability matches `"windows": ["*"]` with `core:default`; review
  it whenever that host's behaviour changes.

## Content Security Policy

All three `tauri.conf.json` files currently set `"csp": null` (no CSP). A CSP for the
Terminal chrome, Desktop Agent dashboard and host is a planned hardening task. Until then,
don't add code to those apps' own UIs that injects HTML (`dangerouslySetInnerHTML`,
`innerHTML`) or loads scripts from remote origins.

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
  can register an app, including its URL and engine binding. Auth arrives with
  [Plan 03](../plans/03-app-directory-roles-permissions.md); until then, treat directory
  records as untrusted input when launching.

## Process spawning

- Engine runtime downloads are verified with SHA-256 against the catalog. Never add a
  download path that skips verification.
- Plugin `LaunchMode::SpawnBinary { binary_name, … }` is passed to `Command::new`, which
  does a `PATH` lookup. Resolve plugin binaries to an absolute path inside the plugin's own
  directory, and reject names containing path separators or `..`.
- Pass untrusted values (`{{url}}`, `{{title}}`, `{{app_id}}`) as discrete arguments or
  env vars — never through a shell (`sh -c`, `cmd /C`).
- Native app launch ([Plan 04](../plans/04-native-app-launch.md)) must be allow-list based
  (`OT_NATIVE_APP_ALLOW_PATHS`).

## Secrets and dependencies

- Never commit secrets. Signing keys live in GitHub Actions secrets ([docs/signing.md](../signing.md)).
- `OT_APPD_TOKEN` and future auth tokens must never be logged or written into scaffolded
  files.
- New dependencies: prefer well-maintained crates and packages; check the licence fits the
  repo's [LICENSE](../../LICENSE). Dependency auditing (`cargo-deny`, `npm audit`) is a
  planned CI addition.

## Reporting

When a change touches any boundary above, say so in the PR description and state how the
input is validated.
