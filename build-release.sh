#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────
# Code Explorer — Release Build Script
#
# Produces:
#   1. code-explorer CLI binary          → target/release/code-explorer(.exe)
#   2. Code Explorer Desktop installer   → target/release/bundle/
#
# Usage:
#   ./build-release.sh          # build both CLI + Desktop
#   ./build-release.sh cli      # CLI only
#   ./build-release.sh desktop  # Desktop app only
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR]${NC}   $*"; exit 1; }

MODE="${1:-all}"

# ─── Pre-flight checks ──────────────────────────────────────────────

info "Checking prerequisites..."

command -v cargo >/dev/null 2>&1 || err "cargo not found. Install Rust: https://rustup.rs"

if [[ "$MODE" == "all" || "$MODE" == "desktop" ]]; then
    command -v npm >/dev/null 2>&1 || err "npm not found. Install Node.js: https://nodejs.org"
    command -v cargo-tauri >/dev/null 2>&1 || {
        warn "cargo-tauri not found, installing..."
        cargo install tauri-cli --locked
    }
fi

ok "Prerequisites OK"

# ─── Step 1: Build CLI ──────────────────────────────────────────────

if [[ "$MODE" == "all" || "$MODE" == "cli" ]]; then
    info "Building CLI (release)..."
    cargo build --release -p code-explorer-cli

    # Locate binary
    if [[ -f "target/release/code-explorer.exe" ]]; then
        CLI_BIN="target/release/code-explorer.exe"
    else
        CLI_BIN="target/release/code-explorer"
    fi

    CLI_SIZE=$(du -h "$CLI_BIN" | cut -f1)
    ok "CLI binary: $CLI_BIN ($CLI_SIZE)"
fi

# ─── Step 2: Build Desktop ──────────────────────────────────────────

if [[ "$MODE" == "all" || "$MODE" == "desktop" ]]; then
    info "Installing frontend dependencies..."
    cd crates/code-explorer-desktop/ui
    npm install --silent
    cd "$ROOT"

    info "Building Desktop app (release)..."
    cd crates/code-explorer-desktop
    cargo tauri build 2>&1 | tail -20
    cd "$ROOT"

    # Find installer output
    BUNDLE_DIR="target/release/bundle"
    if [[ -d "$BUNDLE_DIR" ]]; then
        ok "Desktop installers:"
        find "$BUNDLE_DIR" -type f \( -name "*.msi" -o -name "*.exe" -o -name "*.dmg" -o -name "*.AppImage" -o -name "*.deb" \) 2>/dev/null | while read -r f; do
            SIZE=$(du -h "$f" | cut -f1)
            echo -e "       $f ($SIZE)"
        done
    else
        warn "Bundle directory not found at $BUNDLE_DIR"
    fi
fi

# ─── Summary ─────────────────────────────────────────────────────────

echo ""
echo -e "${GREEN}════════════════════════════════════════${NC}"
echo -e "${GREEN}  Build complete!${NC}"
echo -e "${GREEN}════════════════════════════════════════${NC}"

if [[ "$MODE" == "all" || "$MODE" == "cli" ]]; then
    echo -e "  CLI:     ${CYAN}${CLI_BIN:-target/release/code-explorer}${NC}"
fi
if [[ "$MODE" == "all" || "$MODE" == "desktop" ]]; then
    echo -e "  Desktop: ${CYAN}target/release/bundle/${NC}"
fi
echo ""
