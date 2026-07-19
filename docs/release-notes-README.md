# Before you install — these builds are unsigned

The installers in this release are **not code-signed or notarized**. That's
a known, current limitation of this project's release pipeline — not
malware, not a broken download. Your OS will still warn you, because that's
exactly what an unsigned installer looks like to Gatekeeper (macOS) and
SmartScreen (Windows). This file explains what you'll see and how to get
past it, if you trust this source.

**Only proceed if you downloaded this from the official GitHub Releases
page for this repository.** These steps bypass OS-level publisher
verification — don't apply them to installers from anywhere else.

## macOS

After moving `desktop-agent.app` (or `one-terminal.app`) to `/Applications`
and double-clicking it, you'll see:

> "desktop-agent" is damaged and can't be opened. You should move it to the
> Trash.

This is **not** actual file corruption. Modern macOS (Ventura and later)
shows this specific message — instead of the older "unidentified
developer, open anyway" prompt — for a downloaded app with no valid code
signature at all, once Gatekeeper sees its quarantine flag.

To open it anyway, strip the quarantine attribute in Terminal:

```sh
xattr -cr /Applications/desktop-agent.app
```

(substitute the correct `.app` path if you installed `one-terminal.app`
instead). Then launch it normally.

## Windows

The installer (`.msi`) will trigger:

> Windows protected your PC
> Microsoft Defender SmartScreen prevented an unrecognized app from
> starting.

Click **More info**, then **Run anyway**. This prompt appears because the
installer isn't signed by a certificate Windows recognizes — not because
anything was flagged as malicious.

## Why aren't these signed?

Code signing requires a paid Apple Developer certificate and a Windows
code-signing certificate, which haven't been provisioned for this project
yet. Once they are, these warnings go away for future releases. See
[`docs/signing.md`](https://github.com/griffithtp/one-terminal/blob/main/docs/signing.md)
in the repository if you want the technical details or are setting up
signing yourself.
