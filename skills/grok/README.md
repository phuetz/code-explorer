# Grok skill (Code Explorer)

Canonical copy of the Grok-authored `code-explorer` skill.

Install (idempotent, will not overwrite a different existing file):

```bash
./scripts/install-grok-skill.sh
```

Verified clients for `code-explorer mcp-install --client`: claude, codex, claude-desktop, cursor, vscode. Grok is not a `--client` value; Grok discovers `~/.grok/skills/<name>/SKILL.md` on the **next** session.

Do not use this installer to mutate Code Buddy 2.1 packaging.
