# Cross-platform conventions (Windows + macOS)

OneTerminal ships on **Windows (WebView2)** and **macOS (WKWebView)**; Linux must at
least compile. CI currently builds and tests on Linux only, so a branch that is wrong on
Windows or macOS will not be caught automatically — reviewers (human or AI) must check.

## Conditional compilation

- Every `#[cfg(target_os = "macos")]` needs a matching `#[cfg(not(target_os = "macos"))]`
  (or explicit Windows/Linux branches) — even if the other branch is a documented no-op
  like `let _ = (app, event);`.
- For three-way splits use the existing pattern:
  `macos` / `windows` / `all(unix, not(target_os = "macos"))`.
- Check compilation for the other OS without its hardware:
  `rustup target add x86_64-pc-windows-msvc && cargo check --target x86_64-pc-windows-msvc -p <crate>`
  (or `aarch64-apple-darwin` from Windows). Build scripts or native deps may need the
  real platform; say so in the PR if you couldn't check.

## Paths and directories

| Purpose      | macOS                                         | Windows                          | Linux                                |
| ------------ | --------------------------------------------- | -------------------------------- | ------------------------------------ |
| User config  | `~/Library/Application Support/one-terminal/` | `%APPDATA%\one-terminal\`        | `$XDG_CONFIG_HOME` or `~/.config`    |
| Engine cache | `…/Application Support/one-terminal/engines`  | `%APPDATA%\one-terminal\engines` | `$XDG_DATA_HOME` or `~/.local/share` |

- Build paths with `PathBuf::join`, never string concatenation or hard-coded `/` / `\`.
- Don't compare paths as strings; Windows is case-insensitive and may use `\\?\` prefixes.
- File names must be valid on Windows: no `: * ? " < > |`, no trailing dot/space,
  no reserved names (`CON`, `NUL`, …) — matters for saved dashboards named by users.
- Line endings: generated files and parsers must tolerate `\r\n` (Git for Windows may
  check out with CRLF). Don't rely on symlinks in the repo.

## Processes

- Child processes are stopped with `taskkill /PID [/F]` on Windows and `kill -TERM|-KILL`
  elsewhere (`desktop-agent` `lib.rs`). New spawning code must handle both, and avoid
  flashing a console window on Windows (`CREATE_NO_WINDOW`) for background helpers.
- Executables are `name.exe` on Windows; use `std::env::consts::EXE_SUFFIX`.

## Windows, webviews and input

| Topic                 | macOS (WKWebView)                                                                          | Windows (WebView2)                                                                                         |
| --------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| Frameless resize      | Resizes from edges even over child webviews — popouts run edge to edge (`POPOUT_EDGE = 0`) | Child webview covering the edge swallows the resize hit-test — keep a bare 4 px border (`POPOUT_EDGE = 4`) |
| Dock / taskbar reopen | `RunEvent::Reopen` — un-minimise Terminals ourselves (#71)                                 | No equivalent event                                                                                        |
| First-click drag      | Inactive window may eat the first click — see #71                                          | —                                                                                                          |
| Engine runtime        | System WebKit, one per process                                                             | Fixed-version WebView2 runtime selected per process                                                        |
| Primary modifier      | ⌘ (`metaKey`)                                                                              | Ctrl (`ctrlKey`)                                                                                           |
| Window controls       | Traffic lights on the left                                                                 | Min / max / close on the right                                                                             |

- Keybindings are registered with Tauri's platform-neutral `CmdOrCtrl+K` form
  (`src/commands/`). Never register a raw `Cmd+…` or `Ctrl+…` for a primary-modifier
  shortcut. Note `keyboardListener.ts` maps **both** `metaKey` and `ctrlKey` to
  `CmdOrCtrl`, so on macOS Ctrl+K also fires ⌘K's command. Show the platform's glyph
  (⌘ vs Ctrl) in menus and the command palette.
- Test any change to window geometry, focus, minimise/restore, z-order, drag or popout
  glue on both OSes, and note in the PR which ones you tested.
- Web APIs differ: check WebKit support before using a newer DOM/CSS feature, since
  WKWebView tracks Safari, not Chromium.

## Signing and packaging

macOS needs signing + notarisation and Windows needs code signing for release builds — see
[docs/signing.md](../signing.md). New bundled binaries or sidecars (like the embedded
`one-terminal` in the Desktop Agent bundle) must be included in both platforms' signing.

## Known quirks log

Add a row here whenever you fix an OS-specific bug, with the PR number, so the knowledge
isn't lost:

| PR  | OS    | Quirk                                                                                                     |
| --- | ----- | --------------------------------------------------------------------------------------------------------- |
| #71 | macOS | Minimised Terminal couldn't be restored from the Dock with a popout open; first click didn't start a drag |

## Known open issues

Not yet fixed — don't copy these patterns, and remove a row when it's fixed:

| OS      | Where                                                                              | Issue                                                                                                      |
| ------- | ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Windows | root `package.json`: `dev:terminal:pin`, `dev:desktop-agent:pin`, `dev:*:electron` | POSIX `VAR=value cmd` and `$INIT_CWD` syntax; fails from cmd/PowerShell (use `cross-env` or a Node script) |
| Windows | `scripts/check-template-drift.ts`, `scripts/create-migration.ts`                   | Shell out to Unix `diff` (and `tail`/`head`); work only under Git Bash or WSL                              |
