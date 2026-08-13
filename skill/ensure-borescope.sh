#!/usr/bin/env bash
# ensure-borescope.sh — install borescope and optionally install the skill file
# Usage:
#   ./ensure-borescope.sh              # install binary only
#   ./ensure-borescope.sh --skill      # also install the Claude Code skill
#   ./ensure-borescope.sh --cursor     # also install the Cursor rule

set -euo pipefail

SKILL_FLAG=0
CURSOR_FLAG=0

for arg in "$@"; do
    case "$arg" in
        --skill)  SKILL_FLAG=1 ;;
        --cursor) CURSOR_FLAG=1 ;;
    esac
done

# ── 1. Check for borescope ────────────────────────────────────────────────────

if command -v borescope &>/dev/null; then
    echo "borescope already installed: $(borescope --version 2>/dev/null || echo 'unknown version')"
else
    echo "borescope not found — installing via cargo..."
    if ! command -v cargo &>/dev/null; then
        echo "error: cargo not found. Install Rust: https://rustup.rs" >&2
        exit 1
    fi
    cargo install borescope 2>/dev/null || {
        # Fall back to building from source if in the repo
        if [ -f "Cargo.toml" ] && grep -q 'name = "borescope"' Cargo.toml 2>/dev/null; then
            echo "Building from source..."
            cargo build --release
            export PATH="$PWD/target/release:$PATH"
        else
            echo "error: cannot install borescope. Clone the repo and run 'cargo build --release'" >&2
            exit 1
        fi
    }
    echo "borescope installed: $(borescope --version 2>/dev/null || echo 'ok')"
fi

# ── 2. Index the current repo ─────────────────────────────────────────────────

if [ -f ".git/HEAD" ] || git rev-parse --git-dir &>/dev/null 2>&1; then
    echo ""
    echo "Phase 1: indexing symbols (fast)..."
    borescope index --no-git
    echo "Phase 1 complete. paths / callers / map / explain are ready."
    echo ""
    echo "Phase 2: layering in git history (background)..."
    borescope index --git &
    echo "Phase 2 running in background (PID $!). hotspots / smells / age / coupled will populate shortly."
else
    echo "Not in a git repository — skipping index. Run 'borescope index' after navigating to your repo."
fi

# ── 3. Install skill file (optional) ─────────────────────────────────────────

if [ "$SKILL_FLAG" -eq 1 ]; then
    SKILL_DIR="$HOME/.claude/skills"
    mkdir -p "$SKILL_DIR"
    borescope skill > "$SKILL_DIR/borescope.md"
    echo ""
    echo "Claude Code skill installed: $SKILL_DIR/borescope.md"
fi

if [ "$CURSOR_FLAG" -eq 1 ]; then
    CURSOR_DIR=".cursor/rules"
    mkdir -p "$CURSOR_DIR"
    {
        printf -- "---\ndescription: Borescope code navigation — use before reading files\nglobs: [\"**/*\"]\nalwaysApply: true\n---\n\n"
        borescope skill
    } > "$CURSOR_DIR/borescope.mdc"
    echo ""
    echo "Cursor rule installed: $CURSOR_DIR/borescope.mdc"
fi

echo ""
echo "Done. Try: borescope map --weight hotspot"
