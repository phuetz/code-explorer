#!/usr/bin/env bash
# The code-explorer skill lives in three places on purpose:
#   skills/code-explorer          -> Claude Code plugin (+ `npx skills add phuetz/code-explorer`)
#   .claude/skills/code-explorer  -> auto-loaded when this repo is the working project
#   .codex/skills/code-explorer   -> Codex (frontmatter differs: no allowed-tools/argument-hint, codex client)
# The first two must stay byte-identical. Run from the repo root.
set -euo pipefail
if ! diff -q skills/code-explorer/SKILL.md .claude/skills/code-explorer/SKILL.md >/dev/null; then
  echo "skills/code-explorer/SKILL.md and .claude/skills/code-explorer/SKILL.md differ — copy one over the other" >&2
  exit 1
fi
echo "skills in sync"
