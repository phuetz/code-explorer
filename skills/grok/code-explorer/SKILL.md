---
name: code-explorer
description: Index and query a local Code Explorer knowledge graph (symbols, impact, architecture). Use when the user asks to map a repo, find definitions/callers, or query the graph. Do not use for ordinary single-file edits, git, or tests. Requires a local `code-explorer` binary. Do not call `ask`, `--rerank`, `--llm-enrich`, or `--embeddings` unless the user opts in (those can use a paid LLM).
metadata:
  author: Grok
  short-description: Local Code Explorer CLI (no auto paid LLM)
  compatibility: Requires code-explorer on PATH
---

# Code Explorer (local CLI)

Binary: `code-explorer` on PATH (`code-explorer --version`). Do not install a second copy when the binary is already present.

This skill is **opt-in**. Do not index every repo you touch.

## Workflow

1. `code-explorer status` in the target repo. Unindexed → `Status: NOT INDEXED`.
2. Index only when needed: `code-explorer analyze [PATH]`  
   Useful flags from `--help`: `--incremental`, `--skip-git`, `--max-files <N>`, `--exclude <PATTERN>`, `--no-docs`.  
   Do **not** pass `--embeddings`, `--llm-enrich`, or `--force` unless asked.
3. Search: `code-explorer query "…" --limit 5` (optional `-r/--repo`). Do **not** pass `--rerank` unless asked (needs `~/.codeexplorer/chat-config.json`).
4. Keep citations: quote paths and snippets from the CLI output. On doctor/query errors, paste the real stderr and stop.

`code-explorer doctor [PATH]` (optional `--json`) diagnoses a broken index; exit 1 means unusable.

## MCP

`code-explorer mcp` — stdio MCP.  
`code-explorer mcp-install --help` clients: `claude`, `codex`, `claude-desktop`, `cursor`, `vscode`, `both`, `all`. **Grok is not a `--client`.** A new Grok session is required to see MCP/skills after files appear on disk.

## Do not

- Invent flags. Re-run `<cmd> --help` if unsure.
- Auto-login or auto-`ask` (OAuth/LLM).
- Treat this skill as mandatory for all coding work.
