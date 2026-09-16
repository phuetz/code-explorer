# Grok skill (Code Explorer)

https://github.com/phuetz/code-explorer

Install into the user Grok skill directory (idempotent; skips if any dest file differs):

```bash
./scripts/install-grok-skill.sh --target grok
./scripts/install-grok-skill.sh --target codex
```

`--force` copies after a timestamped backup of dest. Extra files in dest are kept.

Grok discovery: `grok inspect --json` (field `skills`). MCP `--client grok` does not exist. Claude/Buddy skill install is not claimed by this script.
