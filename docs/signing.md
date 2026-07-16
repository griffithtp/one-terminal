# Release signing

`.github/workflows/release.yml` builds and uploads `one-terminal` (standalone
installer) and `desktop-agent` (the single-download bundle — embeds
`one-terminal` as a sidecar and runs App Directory in-process) on every
`v*` tag push, as **draft** GitHub Releases. A human must review and publish
the draft; nothing goes live automatically.

**`release.yml` does not currently reference these secrets at all** — they
must be added back into the workflow's `env:` blocks yourself once you've
provisioned them (see Step 4 below for why). Without them, the workflow
produces unsigned installers, which trigger Gatekeeper (macOS) / SmartScreen
(Windows) warnings for end users but are fine for testing the pipeline
itself. Set these up, then add the env vars back, before publishing a
release users are expected to install.

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
6. **Add the env vars back into `release.yml`'s two `tauri-action` steps**:
   `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
   `APPLE_ID`, `APPLE_ID_PASSWORD`, `APPLE_TEAM_ID` (each as
   `${{ secrets.X }}`, same pattern as `GITHUB_TOKEN`). They were deliberately
   removed rather than left pointing at unset secrets — see "Why these
   aren't just always present" below. `tauri-action` handles signing and
   notarization automatically once these are set — no changes needed to
   `tauri.conf.json`.

## Windows: code signing

1. Obtain a code-signing certificate (`.pfx`) from a CA (e.g. DigiCert,
   Sectigo) or your org's existing one.
2. Base64-encode it (PowerShell:
   `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx")) | Set-Clipboard`)
   → `WINDOWS_CERTIFICATE`.
3. Its password → `WINDOWS_CERTIFICATE_PASSWORD`.
4. Add `WINDOWS_CERTIFICATE` / `WINDOWS_CERTIFICATE_PASSWORD` back into
   `release.yml`'s two `tauri-action` steps, same as the Apple vars above.

## Why these aren't just always present in `release.yml`

`${{ secrets.X }}` evaluates to an **empty string**, not "unset", when a
secret doesn't exist in the repo. Tauri's macOS bundler checks whether
`APPLE_CERTIFICATE` is _present_ (`Ok("")`), not whether it's _non-empty_ —
so leaving `APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}` in the
workflow with no secret configured makes it attempt to import an empty
string as a keychain certificate and hard-fail with
`SecKeychainItemImport: One or more parameters passed to a function were not
valid` — confirmed, this is exactly what broke the `macos-latest` leg on the
first real tag-push test. Since no signing secrets are configured in this
repo yet, the env vars are omitted entirely for now rather than left
pointing at empty secrets. Add them back only once the actual secrets exist.

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

## Release structure

A dedicated `create-release` job creates one draft GitHub Release up front
(via the GitHub API directly, not `tauri-action`); each of the three OS jobs
then attaches its `one-terminal`/`desktop-agent` assets to that same release
via `releaseId` rather than independently finding-or-creating a release by
tag name. This matters because three parallel matrix legs racing to
find-or-create a release for the same tag is a real create/create race —
`releaseId` sidesteps it. Each OS job also stamps `tauri.conf.json`'s
`version` field from the pushed tag before building (it's `0.1.0` in the
repo by default; without this every release would ship installers and app
metadata labeled `0.1.0` regardless of the tag) — any semver pre-release
suffix (e.g. `-test`) is stripped before writing it, since MSI's
`ProductVersion` field is strictly numeric `major.minor.build` and hard-fails
on a suffix like `-test` (confirmed on the first real test run).

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
- Tested against a real `v0.0.1-test` tag push: found and fixed the MSI
  version-format bug and the empty-secret codesigning bug documented above.
  The `create-release` → matrix `releaseId` handoff itself (does it actually
  avoid the cross-job race) hasn't been specifically confirmed yet — re-test
  with a fresh tag after these fixes and check the release ends up with all
  six assets (two apps × three platforms) rather than duplicates or a
  partial set.
