# Code Explorer + lm-resizer

Code Explorer and lm-resizer solve two different parts of the same agent-context problem:

- Code Explorer builds a local code graph so Claude Code, Codex, Cursor, or VS Code can ask targeted repository questions.
- lm-resizer compresses supported large tool outputs such as JSON, logs, diffs, search results, and source snippets. When an offload transform applies, it stores recoverable originals behind CCR hashes.

Use them together when an agent needs repository understanding without flooding the model context.

## Recommended Setup

Build and install both tools:

```bash
# Code Explorer
cd /path/to/code-explorer
cargo build --release -p code-explorer-cli

# Optional: detect extensionless scripts during indexing
cargo build --release -p code-explorer-cli --features magika-detect

# lm-resizer
cd /path/to/lm-resizer
cargo build --release
```

Index the target repository:

```bash
cd /path/to/project
code-explorer analyze .
```

Install MCP servers:

```bash
# Claude Code project config + Codex user config
code-explorer mcp-install --client both --scope project
lm-resizer install --client claude --scope project
lm-resizer install --client codex --scope global

# Or include Cursor and VS Code too
code-explorer mcp-install --client all --scope project
lm-resizer install --client all --scope project
```

Restart the agent after installation.

## Division Of Responsibility

Use Code Explorer for repository selection:

```text
What calls PaymentService?
What breaks if I change handleLogin?
Show the context around UserRepository.
Which files implement authentication?
```

Use lm-resizer when the selected context is large and structurally compressible:

```text
Compress this graph/context output and keep the original retrievable.
Retrieve the original for CCR hash <hash>, when the compression result includes one.
```

The agent can call:

- `context`, `impact`, `query`, `trace-files`, and related Code Explorer MCP tools to select the right code facts.
- `lm_resizer_compress` to shrink logs, diffs, JSON, search results, or source snippets selected from the code graph.
- `lm_resizer_retrieve` to recover original offloaded content by CCR hash when the compressed view is not enough.

Plain text graph summaries may be returned unchanged. JSON may be minified without producing a CCR key. That is intentional: lm-resizer avoids adding retrieval indirection when a local rewrite is enough.

## Why Not Merge Them?

Keep the tools separate:

- Code Explorer owns code intelligence, parsing, graph construction, and repository queries.
- lm-resizer owns context compression, token reduction, and CCR retrieval.

That separation keeps Code Explorer's indexing deterministic and keeps lm-resizer usable for any large tool output, not only code graphs.

## Magika/ONNX Note

Both projects keep ML-backed detection off the hot path:

- Code Explorer only uses Magika/ONNX when built with `--features magika-detect`, and only for extensionless files.
- lm-resizer uses deterministic local detection by default; its README documents opt-in Magika behavior separately.

This means the default workflow stays fast and local, while extensionless scripts can still be handled when explicitly enabled.

## Social Post Angle

Short version:

```text
Code Explorer decides which code context matters.
lm-resizer makes supported large payloads cheaper to pass around, and uses CCR retrieval when offload is worthwhile.

Together: graph-first repository understanding + recoverable compression for Claude Code and Codex.
```
