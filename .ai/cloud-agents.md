# Cloud coding agents

How to set up assistants that work in their own cloud machine and open PRs — GitHub
Copilot's coding agent, Devin, OpenAI Codex (cloud), Cursor background agents and
similar. Everything points at the same sources as local assistants: `AGENTS.md`,
`docs/conventions/`, `.ai/agents/` and `.ai/skills/`. Don't copy their content into a
tool's settings — link to the file, so there's one thing to keep up to date.

## Machine setup

All of them run Linux. One script prepares the machine:

```sh
bash scripts/agent-setup.sh     # Tauri system packages, Rust + rustfmt/clippy, npm ci,
                                # stub dist dirs, then warms the Rust build cache
```

Node.js 22+ must already be installed (the script checks). Set `OT_SETUP_SKIP_WARM=1` to
skip the ~several-minute cache warm when the tool has a short setup timeout.

| Tool                    | Where the setup goes                                                                                                                                     |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GitHub Copilot          | `.github/workflows/copilot-setup-steps.yml` (runs the script). Only used by the cloud agent on github.com — delete it if you only use Copilot in the IDE |
| Devin                   | Repository setup → run `bash scripts/agent-setup.sh`; set Node 22 as the default Node                                                                    |
| Codex (cloud)           | Environment → setup script: `bash scripts/agent-setup.sh`                                                                                                |
| Cursor background agent | Environment install command: `bash scripts/agent-setup.sh`                                                                                               |

Linux can build, lint and run every test, but can't exercise Windows- or macOS-only
behaviour. Agents must say so in their PR and leave per-OS manual checks for a person
(see the feature workflow, step 5). CI runs Rust on all three OSes.

## Instructions, roles and workflows

| Tool          | Context                       | Roles (`.ai/agents/`)       | Workflows (`.ai/skills/`)                                  |
| ------------- | ----------------------------- | --------------------------- | ---------------------------------------------------------- |
| Copilot       | `AGENTS.md` (root and nested) | `.claude/agents/` (VS Code) | `.claude/skills/`                                          |
| Devin         | `AGENTS.md`                   | follow the role file        | a playbook that says "follow `.ai/skills/<name>/SKILL.md`" |
| Codex, Cursor | `AGENTS.md` (nested ones too) | follow the role file        | follow the skill file                                      |

### Devin specifics

- **Knowledge:** add one entry — "Before any task in this repo, read `AGENTS.md` and the
  nested `AGENTS.md` for the area; conventions are in `docs/conventions/`; use the roles
  in `.ai/agents/` and the workflows in `.ai/skills/`." Keep repo facts in the repo, not
  in Knowledge, so they're versioned and reviewed.
- **Playbooks:** create one per workflow you use (`feature`, `framework-upgrade`) whose
  body is just: "Follow `.ai/skills/<name>/SKILL.md` exactly, including its gates."
- **Gates:** the feature workflow stops for approval; in Devin, that means posting the
  plan/design and waiting for a reply in the session before continuing.

## Rules for every cloud agent

- Work on a branch and open a PR; never push to `main`.
- Run the done checklist from `AGENTS.md` before opening the PR, and paste the results.
- Generated files (`templates/`, `fdc3-plugin.js`, AI adapters) come from their
  generators — CI's drift checks catch hand edits.
- PRs touching paths in `.github/CODEOWNERS` request the owner's review automatically.

Check each tool's current documentation when setting it up — file locations and setting
names change faster than this repo.
