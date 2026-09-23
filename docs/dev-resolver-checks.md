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

## Rust qualified calls

`crate::`, `self::`, `super::`, a child module, a `use` alias, and a path-dependency crate name are resolved to the module file, then to the callable of that name in that file. Reason: `rust-path:…`. Confidence 0.95.

A path-shaped qualifier (`::`, `crate`, `self`, `super`, a resolved module whose callable is missing) is never tied to a same-named function in another module. If the path matches several distinct callable nodes (for example both `foo.rs` and `foo/mod.rs` define `run`), no Calls edge is created. The caller node lists the path in `ambiguousCalls`, and `code-explorer context` prints `Ambiguous calls`.

`Type::fn()` and `Self::fn()` do not fall through to import-scoped or global matching. The same-file tier (reason `same-file`, confidence 1.0) is kept only when the qualifier is `Self` inside the impl that contains the call, a type or trait that has an `impl`/`trait` block with `fn` in this file, or an inline `mod` that contains `fn`. `Vec::new()`, `fs::write` and `serde_json::from_str` do not attach to a local homonym.

A type imported by `use path::Type` (or `use path::{Type}`), and a path whose last segment is a type (`path::Type::fn()`), resolve to that type's method in the module file (reason `rust-type`, confidence 0.95). The same applies to `Trait::m` and `<T as Trait>::m`. `super` / `self` inside an inline module use that module's scope: the first `super` is the enclosing file, not the parent file.

A `use` inside a function or a `{ … }` block applies only in that block. Two `use crate::… as g` in two functions stay apart: the call takes the alias of the block that contains it, and never the other function's alias. A module-level `use` is visible in that module only. An extern-crate path (`use dep::mod::Type`) is absolute even inside a child module. An inline child sees a parent `use` when the child has a module-level `use super::*` (each step of a nested child needs its own glob). That glob is not expanded name by name: the parent's bindings are reopened, which is what `mod tests { use super::*; Gate::open() }` needs. Without `use super::*`, the child does not see the parent's `use`. A local `mod`, `struct`, `enum`, `trait`, `type` or `union` of the same name hides the inherited binding; a `fn` does not (`fn git` does not hide module `git`).

`#[path = "file.rs"]` applies only to the `mod name;` it precedes, relative to the file that declares it. It does not fall back to `name.rs` or `name/mod.rs`. A missing path file stays unresolved. A `#[path]` nested in `mod parent { … }` does not retarget a file-level `mod` of the same name. `#[path]` on a file module declared inside an inline module is not followed (`crate::legacy::gate` stays unresolved).

When several file-level `mod name;` stay possible, there is no 0.95 Calls edge: the caller lists the path in `ambiguousCalls`. `#[cfg(test)]` against `#[cfg(not(test))]` is exact: a call that is not lexically under `cfg(test)` resolves to the production module, and a call inside `cfg(test)` resolves to the test module. Any other `cfg` (`unix`, `windows`, `feature`) that leaves more than one file-level `mod name;` is ambiguous, even if only one of the files defines the function. That rule is only about file-level `mod name;` declarations: an inline module is not part of it (limit L1 below). A single declaration, even behind an unknown `cfg`, stays a normal link. `#[cfg_attr(…, path = "…")]` is not read.

Workspace `members` globs (`crates/*`) are expanded and `exclude` is honored. `dep.workspace = true` follows `[workspace.dependencies]` when that entry has a `path`. A resolved module that does not define the function follows `pub use` (depth at most 8) before staying unresolved.

Not handled: glob `use foo::*` as a name import, except `use super::*` which reopens the parent bindings (a `pub use path::*` re-export is still followed), `use super::*` written inside a function rather than at module level, macro arguments (`println!(…, gate::f())` is not a call), `#[cfg_attr(…, path = "…")]`, a `#[path]` file module nested under an inline module, and crates.io dependencies that are not path dependencies. Re-exports (`pub use`) are resolved in the production configuration. Two functions of the same name in one file still share a single Function node (pre-existing id scheme); a path that lands on that file links that node. A fixture that omits the files of the called types cannot show which calls a full repository loses.

Known limits, assumed. On the full LM Resizer and Code Explorer trees the measured impact is zero (the patterns below do not occur on a project type, or only on std types that were never linked):

- Turbofish on the type: `Wrapper::<u8>::new()` is not resolved to `wrap.rs:new`. `Wrapper::new()` and `Wrapper::default()` are. Fixture `cx2-generique`.
- A trait method whose `impl` lives in another file than the type: `Square::render(s)` is not resolved when `impl Render for Square` is in `render.rs` and `Square` is in `shapes.rs`. Same for `Square::from(3)` with `impl From<u32> for Square` in that other file. `<Square as Render>::render` is resolved. Fixture `cx2-impl-ailleurs`.

### Limits L1 to L6 of the qualified Rust resolver

None of these is a regression against the previous resolver: where the edge is wrong, that binary produced the same edge at confidence 1.0 (`same-file`), or it produced no edge either. L1 and L2 are the first fixes to make later. The `cfg` sentence above is true only for file-level `mod name;`.

- **L1. Inline modules are not part of the ambiguity rule.** The first inline `mod name { … }` wins. Its `cfg` is not read, and neither a second inline module of the same name nor a file-level `mod name;` makes the call ambiguous. A platform stub (`#[cfg(not(unix))] mod sys { … }` next to `#[cfg(unix)] mod sys;`) or a `cfg(test)` inline double therefore gets the 0.95 edge. Fixtures `cx4-inline-cfg`, `cx4-facade-imp`; seen in chrono (`offset/local/mod.rs`, `inner::…`) and anstyle-query (`windows_console::…`). The previous binary produced the same edges at 1.0 (`same-file`).
- **L2. `#[cfg]` on a `use` is not read.** `#[cfg(test)] use crate::testing::clock;` next to `#[cfg(not(test))] mod clock;` links production calls to the test module (0.95). Fixture `cx4-use-cfg-alias`.
- **L3. `<T as Trait>::f` with a `T` foreign to the crate** (tuple, std type) links to the local `impl Trait` method of the same name, typically the caller itself (chrono `time_delta.rs`, serde delegation). The previous binary had the same edge at 1.0.
- **L4. Facade loop by identifier collision.** Two functions of the same name in one file still share a single Function node (`Label:file:name`). A path that lands on that file links that node, which can close a loop through a facade. Fixture `cx4-facade-imp`.
- **L5. Tests in a separate file.** `mod tests;` in its own file is not linked: the call written in that test module stays unresolved. No false Calls edge is created. Fixture `cx4-tests-fichier`.
- **L6. Already out of scope, still unresolved.** `#[cfg_attr(…, path = "…")]` is not read. A `#[path]` file module nested under an inline module stays unresolved (fixture `cx3-path-inline`, including `old()`). Two file-level `mod` declarations under a `cfg` other than the exact `test` / `not(test)` pair produce no 0.95 edge (the path is listed as ambiguous). Glob imports other than a module-level `use super::*` are not expanded as name imports. `pub use` re-exports are resolved in the production configuration. Turbofish on the type (`cx2-generique`) and a trait `impl` in another file than the type (`cx2-impl-ailleurs`) are the two bullets above. Macro arguments are not calls. Crates.io dependencies that are not path dependencies are not followed.

Bare calls (no `::`) keep the previous tiers. Rust `pub` is still not detected as an export, because the export check sees the name rather than the `visibility_modifier`. Do not "fix" that by turning global fuzzy on: homonyms would be linked arbitrarily.

## C# lambdas (minimal APIs)

A parenthesized lambda parameter with an explicit type, `(AnswerCache cache) => cache.Clear()`, is bound to `Clear` on the class, struct, or interface of that name. Reason: `lambda-param:AnswerCache:Clear`. Several types of that name defining the method produce `ambiguousCalls` and no Calls edge.

Not handled: property reads (`answers.Count`), lambdas without parentheses, `var` parameters, and a parameter whose type is only a namespace-qualified name that does not match a class node. Overloads of one method in one file still share one node.

