#!/usr/bin/env bash
# D1 reproduction: the MCP server answers "Repository not found" for a
# repository that `code-explorer analyze` indexed *after* the server started.
#
# Scenario (the one the fleet hits every day):
#   1. an MCP server is already running (editor / agent session),
#   2. the user indexes a git worktree with `code-explorer analyze --force`,
#   3. the very next MCP `context` call fails with "Repository not found",
#      while the CLI answers correctly on the same index.
#
# Everything happens in a throw-away directory with an isolated
# CODE_EXPLORER_HOME, so the developer's real registry is never touched.
#
# Usage:  scripts/d1-repro.sh [path/to/code-explorer]
# Exit:   0 = MCP resolved the repository (bug fixed), 1 = D1 reproduced.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

pick_binary() {
  if [ "$#" -ge 1 ] && [ -n "${1:-}" ]; then
    printf '%s\n' "$1"
    return
  fi
  if [ -n "${CE_BIN:-}" ]; then
    printf '%s\n' "$CE_BIN"
    return
  fi
  for candidate in \
    "$REPO_ROOT/target/debug/code-explorer" \
    "$REPO_ROOT/target/release/code-explorer"; do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  command -v code-explorer
}

CE="$(pick_binary "${1:-}")"
if [ -z "$CE" ] || [ ! -x "$CE" ]; then
  echo "d1-repro: no code-explorer binary found (build one or pass its path)" >&2
  exit 2
fi
echo "d1-repro: binary   = $CE"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/ce-d1-repro.XXXXXX")"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

export CODE_EXPLORER_HOME="$WORK/home"
mkdir -p "$CODE_EXPLORER_HOME"
echo "d1-repro: sandbox  = $WORK"

# ── 1. A small git repository plus a linked worktree ────────────────────────
MAIN="$WORK/main"
mkdir -p "$MAIN/src"
cat > "$MAIN/src/lib.rs" <<'RUST'
pub fn d1_marker_symbol(input: u32) -> u32 {
    d1_helper(input) + 1
}

fn d1_helper(input: u32) -> u32 {
    input * 2
}
RUST
git -C "$MAIN" init -q
git -C "$MAIN" config user.email "qa@example.invalid"
git -C "$MAIN" config user.name "QA"
git -C "$MAIN" add src/lib.rs
git -C "$MAIN" commit -q -m "seed"

WT="$WORK/wt"
git -C "$MAIN" worktree add -q -b d1-branch "$WT"
test -f "$WT/.git" || { echo "d1-repro: expected a worktree .git FILE" >&2; exit 2; }

# ── 2. Start the MCP server BEFORE indexing (long-lived server) ─────────────
IN="$WORK/in"; OUT="$WORK/out"
mkfifo "$IN"
"$CE" mcp < "$IN" > "$OUT" 2> "$WORK/server.err" &
SERVER_PID=$!
exec 3> "$IN"
send() { printf '%s\n' "$1" >&3; }

send '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
sleep 1

# ── 3. Index the worktree while the server is already running ───────────────
"$CE" analyze "$WT" --force > "$WORK/analyze.log" 2>&1 || {
  echo "d1-repro: analyze failed" >&2; sed -n '1,20p' "$WORK/analyze.log" >&2; exit 2;
}
grep -q "Indexing complete" "$WORK/analyze.log" || {
  echo "d1-repro: analyze produced no index" >&2; exit 2;
}

# ── 4. Ask the running MCP server for a symbol of that worktree ─────────────
# Three spellings of the same repository; all three must resolve.
send "$(printf '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context","arguments":{"name":"d1_marker_symbol","repo":"%s"}}}' "$WT")"
send "$(printf '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"context","arguments":{"name":"d1_marker_symbol","repo":"%s/"}}}' "$WT")"
send "$(printf '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"context","arguments":{"name":"d1_marker_symbol","repo":"%s/./"}}}' "$WT")"
sleep 3
exec 3>&-
wait "$SERVER_PID" 2>/dev/null

# ── 5. Verdict ─────────────────────────────────────────────────────────────
python3 - "$OUT" <<'PY'
import json, sys

responses = {}
with open(sys.argv[1], encoding="utf-8", errors="replace") as fh:
    for line in fh:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(msg, dict) and "id" in msg:
            responses[msg["id"]] = msg

failed = []
for rid in (2, 3, 4):
    msg = responses.get(rid)
    if msg is None:
        failed.append((rid, "no response from the MCP server"))
        continue
    if "error" in msg and msg["error"] is not None:
        failed.append((rid, msg["error"].get("message", "")))
        continue
    body = json.dumps(msg.get("result", {}))
    if "d1_marker_symbol" not in body:
        failed.append((rid, "resolved but the symbol was not found"))

if failed:
    print("D1 REPRODUCED — the MCP server does not see a repository indexed after it started:")
    for rid, why in failed:
        print(f"  request {rid}: {why}")
    sys.exit(1)

print("D1 fixed — the MCP server resolved the freshly indexed worktree (3/3 path spellings).")
PY
