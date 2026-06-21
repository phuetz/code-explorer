---
name: code-explorer
description: Use Code Explorer / Code Explorer CLI as Codex's code-intelligence layer before broad manual search. Use when answering code questions, mapping repositories such as /home/patrice/code-buddy, tracing symbols, call graphs, impact, dependencies, MCP setup, documentation generation, or when improving TypeScript/JavaScript/C#/Rust graph accuracy.
---

# Code Explorer CLI Skill

Use Code Explorer first for repository understanding, then read exact files to verify details. This skill is especially important for Code Buddy (`/home/patrice/code-buddy`) because Patrice wants Claude, Codex, and Code Buddy to share the same graph-backed map of the code.

## Binary

From this repository (`/home/patrice/DEV/code-explorer-rs`), prefer:

```bash
cargo run -p code-explorer-cli -- <command>
```

After a release build, use:

```bash
./target/release/code-explorer <command>
```

Use `code-explorer <command>` only when the binary is known to be on `PATH`.

## Default Workflow

1. Identify the target repository path.
   - Current Code Explorer repo: `/home/patrice/DEV/code-explorer-rs`
   - Canonical Linux Code Buddy repo: `/home/patrice/code-buddy`
   - Older DEV copy: `/home/patrice/DEV/code-buddy`
2. Check or create the index:
   - From Code Explorer, run `cargo run -p code-explorer-cli -- analyze <target-repo> --skip-git --incremental`.
   - After a release build, run `(cd <target-repo> && /home/patrice/DEV/code-explorer-rs/target/release/code-explorer status)`.
   - Use `--force` when validating resolver changes or when stale graph output is suspected.
3. Start with graph queries:
   - `cargo run -p code-explorer-cli -- query "authentication flow" --repo <target-repo> --limit 10`
   - `cargo run -p code-explorer-cli -- context <symbol> --repo <target-repo>`
   - `cargo run -p code-explorer-cli -- impact <symbol> --repo <target-repo> --direction both`
   - `cargo run -p code-explorer-cli -- trace-files <symbol> --path <target-repo> --depth 3 --json`
4. Read precise files returned by Code Explorer before editing or making claims.
5. If graph output conflicts with source code, trust source code and note the graph issue.

## TypeScript / JavaScript Graph Checks

For TS/JS resolver work, always run an ambiguity reproducer before broad validation:

```bash
GN="cargo run -p code-explorer-cli --"
rm -rf /tmp/ambi && mkdir -p /tmp/ambi
printf 'export function foo() { return 1; }\n' > /tmp/ambi/a.ts
printf 'export function foo() { return 2; }\n' > /tmp/ambi/c.ts
printf 'import { foo } from "./a.js";\nexport function load() { return foo(); }\n' > /tmp/ambi/b.ts
$GN analyze /tmp/ambi --skip-git --force
grep CALLS /tmp/ambi/.codeexplorer/csv/CodeRelation.csv | grep foo
```

Expected result: `load -> a.ts:foo` with reason `named-import` or another import-scoped reason, not `global`.

Also test the dynamic form:

```ts
export async function load() {
  const { foo } = await import("./a.js");
  return foo();
}
```

## Code Buddy Validation

When a change is meant to help Code Buddy:

```bash
cargo run -p code-explorer-cli -- analyze /home/patrice/code-buddy/src --skip-git --force
cargo run -p code-explorer-cli -- query "autonomous code runner" --repo /home/patrice/code-buddy/src --limit 10
cargo run -p code-explorer-cli -- context runTurnLoop --repo /home/patrice/code-buddy/src
```

Track CALLS confidence by inspecting `.codeexplorer/csv/CodeRelation.csv` under the indexed repo. A useful resolver improvement should reduce low-confidence `global` CALLS edges and increase `named-import` / `import-scoped` edges.

## MCP Setup

For Claude Code or a project-local consumer, install the MCP server from the target project directory:

```bash
cd <target-repo>
/home/patrice/DEV/code-explorer-rs/target/release/code-explorer mcp-install --scope project
```

If no release binary exists, build it first:

```bash
cd /home/patrice/DEV/code-explorer-rs
cargo build --release -p code-explorer-cli
```

## Guardrails

- Do not use Code Explorer output as the sole authority for code edits; verify exact source.
- Do not run destructive git commands in indexed repositories.
- Use `--skip-git` for synthetic fixtures and partial source directories.
- For resolver work, avoid changing validated C#/ASP.NET/EDMX behavior unless the task explicitly targets it.
