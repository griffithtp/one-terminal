# Release signing

`.github/workflows/release.yml` builds and uploads `one-terminal` (standalone
installer) and `desktop-agent` (the single-download bundle — embeds
`one-terminal` as a sidecar and runs App Directory in-process) on every
`v*` tag push, as **draft** GitHub Releases. A human must review and publish
the draft; nothing goes live automatically.

Without the secrets below, the workflow still runs and produces unsigned
installers — useful for testing the pipeline itself — but unsigned builds
trigger Gatekeeper (macOS) / SmartScreen (Windows) warnings for end users.
Set these up before actually publishing a release users are expected to
install.

## Required secrets (repo → Settings → Secrets and variables → Actions)

| Secret                         | Purpose                                            |
| ------------------------------ | -------------------------------------------------- |
| `APPLE_CERTIFICATE`            | Base64-encoded Developer ID Application `.p12`     |
| `APPLE_CERTIFICATE_PASSWORD`   | Password for the `.p12`                            |
| `APPLE_SIGNING_IDENTITY`       | e.g. `Developer ID Application: Your Org (TEAMID)` |
| `APPLE_ID`                     | Apple ID used for notarization                     |
| `APPLE_ID_PASSWORD`            | App-specific password for that Apple ID            |
| `APPLE_TEAM_ID`                | Apple Developer Team ID                            |
| `WINDOWS_CERTIFICATE`          | Base64-encoded code-signing `.pfx`                 |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx`                            |

## macOS: certificate + notarization

1. In Xcode (or the Apple Developer portal), create a **Developer ID
   Application** certificate and export it as a `.p12` with a password.
2. Base64-encode it: `base64 -i cert.p12 | pbcopy` → paste as
   `APPLE_CERTIFICATE`.
3. `APPLE_SIGNING_IDENTITY` is the certificate's common name, exactly as it
   appears in `security find-identity -v -p codesigning`.
4. Generate an app-specific password at <https://appleid.apple.com> (Sign-In
   and Security → App-Specific Passwords) for `APPLE_ID_PASSWORD` — do not
   use your real Apple ID password.
5. `APPLE_TEAM_ID` is on the Apple Developer portal's Membership page.

`tauri-action` handles signing and notarization automatically once these are
set — no changes needed to `tauri.conf.json`.

## Windows: code signing

1. Obtain a code-signing certificate (`.pfx`) from a CA (e.g. DigiCert,
   Sectigo) or your org's existing one.
2. Base64-encode it (PowerShell:
   `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx")) | Set-Clipboard`)
   → `WINDOWS_CERTIFICATE`.
3. Its password → `WINDOWS_CERTIFICATE_PASSWORD`.

## Linux

No code-signing step — Linux installers (`.deb`/`.AppImage`, whichever
`bundle.targets` produces) ship unsigned; this is normal for the ecosystem.

## Auto-updater key (not yet wired up)

[Plan 07](plans/07-deployment-and-hosting.md) Issue 07-D (Tauri
auto-updater config) hasn't landed yet — `TAURI_SIGNING_PRIVATE_KEY` /
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` aren't referenced by `release.yml` for
that reason. When 07-D lands, generate the key pair with:

```sh
npx @tauri-apps/cli@latest signer generate -w ~/.tauri/one-terminal.key
```

Store the private key + password as GitHub secrets (never commit the
private key); the public key goes into `tauri.conf.json`'s `plugins.updater`
config and is safe to commit.

## Known limitations of the current workflow

- **macOS builds `aarch64-apple-darwin` only** (Apple Silicon) — not
  `universal-apple-darwin`. A universal build requires
  `scripts/build-terminal-sidecar.ts` to cross-compile the embedded
  `one-terminal` sidecar for both `aarch64` and `x86_64` before
  `desktop-agent` packages them together, which hasn't been exercised.
  Intel Mac users are unsupported until this is verified. See
  [docs/plans/07-deployment-and-hosting.md](plans/07-deployment-and-hosting.md)
  Issue 07-A.
- Releases are created as **drafts** — publishing is a manual step.
- No Docker/GHCR publishing yet for the hosted `app-directory-server`
  binary — that's [Plan 07](plans/07-deployment-and-hosting.md) Issue 07-B,
  not yet implemented.
