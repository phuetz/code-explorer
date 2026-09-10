# Index freshness

Local fixes verified on 2026-09-08. No external LLM, embeddings, dependency or
network service is needed for these paths.

## Updating after edits

`code-explorer analyze <repo> --incremental` reuses content-addressed parsing
results for unchanged files. New and modified files are parsed; deleted files
are excluded. SHA-256 content comparison detects edits even when size and mtime
are unchanged. `--force` always parses every supported file.

The parsing cache stores nodes, local edges and unresolved imports/calls/heritage
before cross-file enrichment. Each run reconstructs the graph from those inputs.
TODO inventory and non-C# API endpoint extraction now reuse file-owned enrichment
artifacts. Changed/deleted files and their old/new direct graph neighbors are
invalidated; a hash of each scanner's current file-local input also detects
changes introduced by earlier global phases. Deleted artifacts are discarded.
API route ownership is merged in file order across all artifacts, so an unchanged
losing declaration can become the winner after another file removes its route.

Imports, calls, heritage, ASP.NET MVC, schema/config joins, communities, processes
and dead-code classification remain repository-wide. Unresolved names, transitive
re-exports, shared table/config identities, global modularity and process ranking
are not bounded by existing direct graph edges. Their timings remain explicit in
`metrics.json`; `local_enrichment_cache` measures final cache publication. This is
partial parsing plus two locally recomputed enrichments, not a fully bounded
incremental graph update. Scanning, hashing, artifact restoration and global joins
still visit unchanged data; there is no universal latency guarantee.

Graph construction uses ordered maps and file indexes, stable community
tie-breaking, sorted C# annotations/import candidates and stable process ranking.
Byte equality is checked on the fixture and real copies described in the report.
Optional LLM enrichment is outside these checks.
The JSON snapshot format remains compatible; ordered lookup costs O(log n).

`.codeexplorer/parse-cache.bin` contains a SHA-256-protected JSON payload and an
executable fingerprint. Missing, malformed, checksum-invalid or incompatible
caches cause an explicit `Incremental fallback: ... full refresh` message and
full parsing. Cache writes use a unique temporary file and atomic replacement;
cache write failure leaves indexing functional but can prevent reuse next time.
The cache is independent of graph/manifest publication because every entry is
validated against current file contents. An executable change invalidates it.

`.codeexplorer/local-enrichment.bin` stores a checksum-protected payload and an
executable fingerprint. Missing, corrupt or incompatible data causes local phases
to rescan all files. Its per-file input fingerprints make cache publication
independent of the final snapshot, as with the parsing cache.

`.codeexplorer/analyze.json` reports `parsed_files`, `total_files`, `duration_ms`,
`fallback_reason`, `resolution_scope` (still `repository`) and `local_enrichments`.
The latter reports `scanned_files` and `reused_files` for `todos` and `api_surface`.
Snapshots and CSVs are regenerated; consumers rebuild text and adjacency indexes.

The default CLI oracle compares nodes, edges, incoming/outgoing adjacency and
text search, covering callee rename/deletion and corrupt-cache fallback. It only
asserts functional equivalence and parsing counts, never a wall-clock ratio on
the 20-file fixture. A separate ignored test generates 320 Rust files, changes
three, verifies byte equality against a full rebuild and checks incremental time
<=30% of full time:

```sh
cargo test --release -p code-explorer-cli --test incremental_oracle large_repository_timing -- --ignored --nocapture
```

Use a quiet machine and set `TMPDIR` to a workspace QA directory for confinement.
The generated sources and indexes are disposable; no real repository is edited.
The oracle fixes its child processes to two Rayon threads.
The local-enrichment oracle additionally covers direct-neighbor invalidation,
duplicate-route ownership, deletion and corrupt enrichment-cache fallback.
See the [Tranche 2 measurement report](reports/2026-09-07-incremental.md) for exact
snapshot hashes, real-copy scope, timings and remaining global phases.

## Long-lived MCP servers

Each snapshot lookup checks its file length and modification time. A changed,
missing or unreadable snapshot invalidates the cached graph, adjacency indexes
and full-text index together. A replacement during loading gets one bounded
retry. Stable snapshots retain their cached graph and do not reread its contents.

A replacement deliberately preserving both exact length and modification time
cannot be detected with this fingerprint. A generation or content digest is a
future format-level improvement. A transient replacement gap can return an
explicit error; the next request reloads instead of returning stale data.

## Regression checks

- CLI: add an uncommitted source file, update, and query the new symbol.
- CLI: modify only a callee, update its line range, and retain its unchanged caller.
- MCP: replace a snapshot and rebuild graph, adjacency and text-search caches.
- MCP: remove a snapshot and refuse the previously cached result.

Run the affected tests with:

```text
cargo test --workspace
cargo test -p code-explorer-cli --test incremental_oracle --test local_enrichment_oracle
cargo test --release -p code-explorer-cli --test cli_integration cli_incremental_analyze --locked
cargo test --release -p code-explorer-mcp --locked
```

## Related boundaries

`watch` still directly uses the old partial engine and is not repaired by this
CLI parsing cache. `status` compares Git HEAD rather than dirty source content, so its
up-to-date message does not establish worktree freshness. Plain `analyze` still
uses its existing-index shortcut. Use the explicit update command above after
editing; these fixes do not claim relevance-ranking or language-coverage gains.

## Renames, deletions and branch switches

Two incremental paths coexist and they fail differently.

`analyze --incremental` rebuilds the graph from the parsing cache, so a rename,
a deletion or a `git checkout` cannot leave anything behind: the resulting
graph is the one a full re-index of the same state produces. That equivalence
is asserted in `crates/code-explorer-cli/tests/cli_integration.rs` — sorted
node ids **and** relationship ids compared against `analyze --force`, plus a
check that no edge points at a missing node — for a rename, a deleted
directory, and a branch switch there and back. Each case also reads
`parsed_files` from `.codeexplorer/analyze.json` to confirm that only the files
that actually differ were re-parsed.

`watch` uses the in-place engine (`incremental::incremental_update`), which is
where a node can survive its file. Two defects were fixed there:

- **Emptied folders.** `remove_nodes_by_file` deletes a file's nodes and the
  edges touching them, but a folder is not a file: its `Folder` node survived
  with no children and consumers kept showing a directory that no longer
  existed. `KnowledgeGraph::remove_empty_folders` prunes bottom-up, so a chain
  such as `docs/guide/` → `docs/` goes in one pass.
- **Untracked prose.** The manifest held only code files, so a deleted or
  renamed Markdown file kept its `File` and `Section` nodes forever and an
  edited one kept its old headings. Documents now take part in the incremental
  manifest — but only when the index already holds documents, so watching a
  code repository does not start indexing its prose.

A link from an unchanged document to a newly added one is not recreated by the
in-place engine, the same boundary that applies to call resolution from
unchanged files. A full index restores it.

## Telling how stale an index is

`code-explorer doctor <path>` compares the indexed commit with `HEAD` and
reports "index is N commit(s) behind HEAD", "index is on a diverged commit", or
"indexed commit is unknown to this checkout" after a rebase or a pruned branch —
each with the `analyze` command that fixes it. See [doctor.md](doctor.md).
