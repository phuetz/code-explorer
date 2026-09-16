---
name: code-explorer
description: Index and query a local Code Explorer knowledge graph (symbols, impact, architecture). Use when mapping a repo, finding definitions or callers, or querying the graph would beat reading files at random. Skip for ordinary single-file edits, git, or tests. Requires `code-explorer` on PATH. Flags that call an LLM (`ask`, `--rerank`, `--llm-enrich`, `--embeddings`) need the same authorization already required for paid or remote model use; local analyze/query/doctor do not.
metadata:
  author: Grok
  short-description: Local Code Explorer graph index and query
  compatibility: Requires code-explorer on PATH
---

# Code Explorer (local CLI)

Binary: `code-explorer` on PATH (`code-explorer --version`). Do not install a second copy when the binary is already present.

Trigger when a graph lookup helps this task. Do not index an entire tree as a default first step.

## Workflow

1. `code-explorer status` in the target repo. Unindexed → `Status: NOT INDEXED`.
2. Index when the graph is missing or stale: `code-explorer analyze [PATH]`  
   Flags from `--help`: `--incremental`, `--skip-git`, `--max-files <N>`, `--exclude <PATTERN>`, `--no-docs`.
3. Search: `code-explorer query "…" --limit 5` (optional `-r/--repo`).
4. Keep citations from CLI output. On errors, paste real stderr and stop.

`code-explorer doctor [PATH]` (optional `--json`); exit 1 means unusable index.

## MCP

`code-explorer mcp` — stdio.  
`code-explorer mcp-install --help` clients: `claude`, `codex`, `claude-desktop`, `cursor`, `vscode`, `both`, `all`. **No `grok` client.** Grok loads `~/.grok/skills/code-explorer/SKILL.md` via skill discovery (`grok inspect --json` field `skills`).

## Do not

- Invent flags; re-run `<cmd> --help`.
- Call `ask` / `--rerank` / `--llm-enrich` / `--embeddings` without existing model authorization.
- Index every repository as routine.
