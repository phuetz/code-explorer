# Social Post Drafts

## LinkedIn

AI coding agents waste context by re-reading source files one by one.

I built Code Explorer to give Claude Code, Codex, Cursor, and any MCP agent a persistent graph of the whole repository.

Setup:

```bash
code-explorer analyze .
code-explorer mcp-install --client both --scope project
```

Claude Code gets `.mcp.json`; Codex gets the user `config.toml`.

Then ask:

- What calls PaymentService?
- What breaks if I change handleLogin?
- Show me the architecture around authentication.

Instead of reading dozens of files, the agent calls one local MCP tool and gets callers, callees, imports, impact, hotspots, ownership, and documentation context.

Rust. Local. 14 languages. 30 MCP tools. Claude Code, Codex, Cursor and VS Code.
Optional Magika/ONNX detection helps extensionless scripts enter the graph without replacing the fast extension-based path.

Pair it with lm-resizer when JSON, diffs, logs, search results, or source snippets get large: Code Explorer selects the right repository facts, lm-resizer compresses supported payloads and keeps offloaded originals retrievable by CCR hash.

https://github.com/phuetz/code-explorer

## X / Twitter

Claude Code and Codex can stop re-reading your repo from scratch.

Code Explorer indexes your code into a local graph, then exposes it over MCP.

```bash
code-explorer analyze .
code-explorer mcp-install --client both --scope project
```

Claude Code gets `.mcp.json`; Codex gets the user `config.toml`.

Ask: "What breaks if I change PaymentService?"

https://github.com/phuetz/code-explorer

## Short Demo Script

1. Open a real codebase.
2. Run:

```bash
code-explorer analyze .
code-explorer demo
code-explorer mcp-install --client both --scope project
```

3. Restart Claude Code or Codex.
4. Ask:

```text
What calls PaymentService?
What is the blast radius if I change it?
Which files are involved?
```

5. Show that the answer comes from graph tools, not brute-force file reading.

## Code Explorer + lm-resizer

Code Explorer and lm-resizer fit together cleanly:

- Code Explorer selects the repository facts that matter.
- lm-resizer compresses supported large payloads: JSON, diffs, logs, search results, and source snippets.
- CCR hashes keep offloaded originals retrievable when the agent needs the full payload.

```bash
code-explorer analyze .
code-explorer mcp-install --client both --scope project
lm-resizer install --client claude --scope project
lm-resizer install --client codex --scope global
```

Claude Code and Codex get graph-first repository understanding plus compression for supported large payloads.

## French LinkedIn

Les agents IA perdent beaucoup de contexte a relire les fichiers un par un.

Code Explorer pre-indexe le depot complet dans un graphe local, puis l'expose a Claude Code, Codex, Cursor ou n'importe quel client MCP.

Installation:

```bash
code-explorer analyze .
code-explorer mcp-install --client both --scope project
```

Ensuite on peut demander:

- Qu'est-ce qui appelle PaymentService ?
- Qu'est-ce qui casse si je modifie handleLogin ?
- Montre-moi l'impact de ce service.

L'agent ne devine pas: il interroge un graphe persistant du code.

https://github.com/phuetz/code-explorer
