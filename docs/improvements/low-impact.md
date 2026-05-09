# Low Impact / Polish Improvements

## 7. Document `agent.config.json` Canonical Location

**Status:** Not started  
**Scope:** `apps/desktop-agent/README.md`, `apps/desktop-agent/src-tauri/src/config.rs`

The loading precedence (next to binary → `resources/`) is only visible by reading the source code. Production deployments — especially those using packaged Tauri builds — will place the file in the wrong location and get silent fallback-to-defaults behavior.

**Actions:**

- Add a "Configuration" section to `apps/desktop-agent/README.md` documenting:
  - Both search paths and which wins
  - All `OT_*` env var overrides and their types/defaults
  - A minimal `agent.config.json` example with inline comments
- Consider logging the resolved config path at `INFO` level on startup so it's visible in production logs

---

## 8. Add Architecture Decision Records (ADRs)

**Status:** Not started  
**Scope:** New `docs/adr/` directory

Several non-obvious architectural decisions are currently undocumented. Contributors reading the code encounter them without context, leading to incorrect "fixes" or repeated re-litigation of settled decisions.

**Candidate ADRs to write:**

| #   | Decision                                                           | Why it's non-obvious                                                       |
| --- | ------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| 001 | TCP for the FDC3 spoke protocol (not WebSockets)                   | WebSockets are more common; TCP requires a custom framing layer            |
| 002 | `fdc3-plugin` is a single vanilla JS file with no build step       | Keeps it zero-dependency and embeddable in any page without a bundler      |
| 003 | `ot-core` has no Tauri dependency                                  | Allows unit testing engine logic without a Tauri runtime                   |
| 004 | EJS templates are generated, never hand-edited                     | Ensures template and live source stay in sync; eliminates dual-maintenance |
| 005 | Engine manifests use `LaunchMode` enum instead of free-form config | Prevents plugin authors from specifying unsupported launch strategies      |

**Format:** Use a lightweight ADR template — title, status, context, decision, consequences. Each file should be under one page.

---

## 9. Document Error Recovery and Session Resilience

**Status:** Not started  
**Scope:** `apps/desktop-agent/README.md`, `packages/ot-fdc3/README.md`

There is no documented answer to: what happens if the Desktop Agent crashes mid-session? Do TCP spoke clients attempt reconnection? Are pending intents or channel subscriptions lost? Even if the current answer is "undefined — restart required," documenting it explicitly prevents production surprises and sets a clear baseline for future resilience work.

**Actions:**

- Document current behavior on Desktop Agent crash: are spoke clients notified? Do they reconnect?
- Document whether `ot-fdc3` (the Tauri plugin) has any reconnection logic or if that's the caller's responsibility
- If reconnection is not implemented, add a `TODO` in `packages/ot-fdc3/src/` as a code-level marker
- Add a "Resilience" section to `apps/desktop-agent/README.md` with the honest current state
