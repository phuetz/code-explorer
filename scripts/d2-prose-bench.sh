#!/usr/bin/env bash
# D2 measurement: what prose indexing costs and what it buys.
#
# Builds a synthetic prose repository (N Markdown chapters with headings and
# internal links) and indexes it twice with the SAME binary:
#   --no-docs      the behaviour before this change (code files only)
#   (default/auto) prose indexed because the repository is mostly prose
# Reports indexing time, index size on disk, node/edge counts, and checks that
# `context` and `query` actually answer on a heading.
#
# Usage:  scripts/d2-prose-bench.sh [chapters] [path/to/code-explorer]
set -uo pipefail

CHAPTERS="${1:-300}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

pick_binary() {
  if [ -n "${2:-}" ]; then printf '%s\n' "$2"; return; fi
  if [ -n "${CE_BIN:-}" ]; then printf '%s\n' "$CE_BIN"; return; fi
  for candidate in "$REPO_ROOT/target/release/code-explorer" "$REPO_ROOT/target/debug/code-explorer"; do
    [ -x "$candidate" ] && { printf '%s\n' "$candidate"; return; }
  done
  command -v code-explorer
}
CE="$(pick_binary "$@")"
[ -x "$CE" ] || { echo "d2-bench: no code-explorer binary found" >&2; exit 2; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/ce-d2-bench.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
export CODE_EXPLORER_HOME="$WORK/home"
mkdir -p "$CODE_EXPLORER_HOME"

BOOK="$WORK/book"
mkdir -p "$BOOK/chapters" "$BOOK/src"
python3 - "$BOOK" "$CHAPTERS" <<'PY'
import sys, pathlib
root = pathlib.Path(sys.argv[1]); n = int(sys.argv[2])
for i in range(n):
    nxt = (i + 1) % n
    (root / "chapters" / f"chapter-{i:03d}.md").write_text(
        f"# Chapter {i} title\n\nOpening paragraph of chapter {i}.\n\n"
        f"## Section {i} alpha\n\nSome prose about topic alpha.\n\n"
        f"## Section {i} beta\n\nContinue with [chapter {nxt}](chapter-{nxt:03d}.md).\n",
        encoding="utf-8")
(root / "README.md").write_text(
    "# Book\n\nStart at [chapter zero](chapters/chapter-000.md).\n", encoding="utf-8")
(root / "src" / "build.rs").write_text("fn main() {}\n", encoding="utf-8")
PY
git -C "$BOOK" init -q
git -C "$BOOK" config user.email "qa@example.invalid"
git -C "$BOOK" config user.name "QA"
git -C "$BOOK" add -A >/dev/null
git -C "$BOOK" commit -q -m "book"

run_case() {
  local label="$1"; shift
  rm -rf "$BOOK/.codeexplorer"
  local out="$WORK/$label.log"
  local start end
  start=$(date +%s%N)
  "$CE" analyze "$BOOK" --force "$@" > "$out" 2>&1 || { echo "analyze failed"; sed -n '1,15p' "$out"; exit 2; }
  end=$(date +%s%N)
  local ms=$(( (end - start) / 1000000 ))
  local files nodes edges size
  files=$(awk '/^  Files:/ {print $2}' "$out")
  nodes=$(awk '/^  Nodes:/ {print $2}' "$out")
  edges=$(awk '/^  Edges:/ {print $2}' "$out")
  size=$(du -sk "$BOOK/.codeexplorer" | cut -f1)
  printf '%-12s wall %6s ms   files %5s   nodes %6s   edges %6s   index %6s KiB\n' \
    "$label" "$ms" "$files" "$nodes" "$edges" "$size"
  grep -E "^  Documents:" "$out" | sed 's/^/             /'
}

echo "d2-bench: binary   = $CE"
echo "d2-bench: chapters = $CHAPTERS  (+1 README, +1 build.rs)"
echo
run_case "code-only" --no-docs
run_case "with-prose"
echo

echo "Answering on prose after the change:"
"$CE" context "Chapter 42 title" --repo "$BOOK" 2>&1 | sed -n '1,4p' | sed 's/^/  /'
"$CE" query "Section 7 beta" --repo "$BOOK" --limit 3 2>&1 | sed -n '1,5p' | sed 's/^/  /'
