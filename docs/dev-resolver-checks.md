# Resolver checks (maintainers)

Moved out of the Codex skill on 2026-09-10 so the shipped skill stays generic. Run from the Code Explorer repository with `cargo run -p code-explorer-cli -- <command>`.

## TypeScript / JavaScript graph checks

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
cargo run -p code-explorer-cli -- analyze <target-repo>/src --skip-git --force
cargo run -p code-explorer-cli -- query "autonomous code runner" --repo <target-repo>/src --limit 10
cargo run -p code-explorer-cli -- context runTurnLoop --repo <target-repo>/src
```

Track CALLS confidence by inspecting `.codeexplorer/csv/CodeRelation.csv` under the indexed repo. A useful resolver improvement should reduce low-confidence `global` CALLS edges and increase `named-import` / `import-scoped` edges.

