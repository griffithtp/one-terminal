#!/usr/bin/env bash
# Prepares a Linux (Ubuntu/Debian) machine for a cloud coding agent — GitHub Copilot's
# coding agent (.github/workflows/copilot-setup-steps.yml), Devin, Codex, Cursor background
# agents — so it can build, lint and test OneTerminal without guessing.
#
#   bash scripts/agent-setup.sh            # full setup + warm the Rust build cache
#   OT_SETUP_SKIP_WARM=1 bash scripts/agent-setup.sh
#
# Idempotent. Mirrors the Linux leg of .github/workflows/ci.yml; keep them in step.
set -euo pipefail

cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

if command -v apt-get >/dev/null 2>&1; then
  step "Tauri system dependencies"
  SUDO=""
  if [ "$(id -u)" -ne 0 ]; then SUDO="sudo"; fi
  $SUDO apt-get update -y
  $SUDO apt-get install -y --no-install-recommends \
    build-essential pkg-config curl \
    libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
else
  step "No apt-get — skipping system packages (install Tauri's prerequisites for this OS by hand)"
fi

step "Rust toolchain"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi
rustup component add rustfmt clippy

step "Node.js"
node_major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)"
if [ "$node_major" -lt 22 ]; then
  echo "Node.js 22+ is required (found: $(node --version 2>/dev/null || echo none))." >&2
  exit 1
fi

step "npm dependencies"
# --ignore-scripts matches CI: `prepare` would install lefthook git hooks.
npm ci --ignore-scripts

step "Stub frontend dist dirs (Tauri build scripts need them to exist)"
mkdir -p apps/one-terminal/dist apps/desktop-agent/dist apps/tauri-webview-host/dist

if [ "${OT_SETUP_SKIP_WARM:-0}" != "1" ]; then
  step "Warm the Rust build cache (compiles the workspace and its tests)"
  cargo test --workspace --no-run
fi

step "Ready. Read AGENTS.md first; checks: cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, npm run lint, npm run typecheck, npm run test:terminal"
