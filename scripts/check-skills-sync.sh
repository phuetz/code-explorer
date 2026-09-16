#!/usr/bin/env bash
# The Claude/Codex skill lives in three places on purpose:
#   skills/code-explorer          -> Claude Code plugin (+ `npx skills add phuetz/code-explorer`)
#   .claude/skills/code-explorer  -> auto-loaded when this repo is the working project
#   .codex/skills/code-explorer   -> Codex (frontmatter differs: no allowed-tools/argument-hint, codex client)
# The Grok-authored skill lives in two more:
#   skills/grok/code-explorer
#   crates/code-explorer-cli/skills/grok/code-explorer
# plus matching install-grok-skill.sh at repo root and in the crate.
# The first two Claude copies must stay byte-identical. The Grok pair and
# the two installers must stay byte-identical. Run from the repo root.
set -euo pipefail
if ! diff -q skills/code-explorer/SKILL.md .claude/skills/code-explorer/SKILL.md >/dev/null; then
  echo "skills/code-explorer/SKILL.md and .claude/skills/code-explorer/SKILL.md differ — copy one over the other" >&2
  exit 1
fi
if ! diff -rq skills/grok/code-explorer crates/code-explorer-cli/skills/grok/code-explorer >/dev/null; then
  echo "skills/grok/code-explorer and crates/code-explorer-cli/skills/grok/code-explorer differ" >&2
  exit 1
fi
if ! diff -q scripts/install-grok-skill.sh crates/code-explorer-cli/scripts/install-grok-skill.sh >/dev/null; then
  echo "scripts/install-grok-skill.sh and crates/code-explorer-cli/scripts/install-grok-skill.sh differ" >&2
  exit 1
fi
echo "skills in sync"
