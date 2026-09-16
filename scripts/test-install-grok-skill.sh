#!/usr/bin/env bash
# Real installer cases against a temp dest (not the user skill tree).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INS="$ROOT/scripts/install-grok-skill.sh"
chmod +x "$INS"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# unknown argument
set +e
out=$("$INS" --nope 2>&1)
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "unknown arg exit $ec"
echo "$out" | grep -q 'unknown argument' || fail "unknown arg message"

# unsupported target
set +e
out=$("$INS" --target claude --dest "$TMP/x" 2>&1)
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "claude target exit $ec"

# 1. fresh install
dest="$TMP/fresh/code-explorer"
"$INS" --target grok --dest "$dest" | grep -q installed || fail "fresh"
[ -f "$dest/SKILL.md" ] && [ -f "$dest/agents/openai.yaml" ] || fail "fresh files"

# 2. second identical
"$INS" --target grok --dest "$dest" | grep -q 'already installed' || fail "identical"

# 3. personalized SKILL.md — skip, keep yaml from first install
echo 'custom-skill' > "$dest/SKILL.md"
"$INS" --target grok --dest "$dest" | grep -q skip || fail "custom skill skip"
grep -qx 'custom-skill' "$dest/SKILL.md" || fail "skill preserved"
[ -f "$dest/agents/openai.yaml" ] || fail "yaml still there"

# 4. personalized YAML — skip, keep both
mkdir -p "$TMP/yaml/code-explorer/agents"
cp "$ROOT/skills/grok/code-explorer/SKILL.md" "$TMP/yaml/code-explorer/SKILL.md"
echo 'custom-yaml' > "$TMP/yaml/code-explorer/agents/openai.yaml"
"$INS" --target grok --dest "$TMP/yaml/code-explorer" | grep -q skip || fail "custom yaml skip"
grep -qx 'custom-yaml' "$TMP/yaml/code-explorer/agents/openai.yaml" || fail "yaml preserved"

# 5. missing yaml repaired when SKILL matches
mkdir -p "$TMP/miss/code-explorer"
cp "$ROOT/skills/grok/code-explorer/SKILL.md" "$TMP/miss/code-explorer/SKILL.md"
"$INS" --target grok --dest "$TMP/miss/code-explorer" | grep -q installed || fail "repair"
[ -f "$TMP/miss/code-explorer/agents/openai.yaml" ] || fail "yaml repaired"

# extra personal file never deleted
echo extra > "$TMP/miss/code-explorer/NOTES.txt"
"$INS" --target grok --dest "$TMP/miss/code-explorer" | grep -q 'already installed' || fail "extra"
[ -f "$TMP/miss/code-explorer/NOTES.txt" ] || fail "extra kept"

# 6. --force backups and replaces SKILL, keeps extra
echo 'old' > "$dest/NOTES.txt"
force_out=$("$INS" --target grok --dest "$dest" --force)
echo "$force_out" | grep -q backup || fail "force backup: $force_out"
grep -q 'Index and query' "$dest/SKILL.md" || fail "force replaced skill"
[ -f "$dest/NOTES.txt" ] || fail "force kept extra"
ls -d "$dest".bak.* >/dev/null || fail "backup dir"
grep -qx 'custom-skill' "$dest".bak.*/SKILL.md || fail "backup had custom skill"

# 7. --target codex with dest
c="$TMP/codex/lm-notused"
"$INS" --target codex --dest "$TMP/codex/code-explorer" | grep -q 'target=codex' || fail "codex target"

echo "PASS installer tests $TMP"
