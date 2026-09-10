# `code-explorer doctor` — why the tools are not answering

`status` answers "is this directory indexed?". `doctor` answers the question
that actually costs time: *the tools say my repository is missing, or stale, or
half indexed — why, and what do I run?*

```bash
code-explorer doctor                 # the current directory
code-explorer doctor ../other-repo
code-explorer doctor --json          # machine-readable
```

Exit code **0** when the index is usable, **1** when it is not — so `doctor`
can gate a pipeline.

## What it checks

| Check | Question it answers |
|---|---|
| `path` | Does the path exist, what is its canonical form, is it a **linked git worktree** or a plain checkout, which branch? |
| `index` | Is there a `.codeexplorer/`, a readable `meta.json`, a `graph.bin` that is present, non-empty and **actually loadable**? |
| `schema` | Was the index written by this build, by an older one, by a newer one, or before the schema stamp existed? |
| `registry` | Is the repository listed in `~/.codeexplorer/registry.json` under its canonical path, exactly once, with a `storagePath` matching the index on disk? |
| `coverage` | How many code files and prose documents does the repository hold, by extension, and how many did the index actually take? |
| `bulk` | Which directories are large enough to dominate an indexing run, which of them `.gitignore` and the default exclusions already drop, and which one you should pass to `--exclude`? |
| `freshness` | Is the indexed commit HEAD, **N commits behind**, on a diverged branch, or unknown to this checkout (rebase, pruned branch)? |

Every verdict carries a level (`ok` / `warn` / `error`), a summary, details and —
when something is wrong — the exact command that repairs it.

## Reading the output

```
Code Explorer Doctor
  Repository: .
  Resolved:   /home/dev/projects/handbook

  [OK   ] path       /home/dev/projects/handbook
               linked git worktree
               shared git dir: /home/dev/projects/main/.git
               branch: chapter-7
  [OK   ] index      12434 nodes, 28073 edges (18.1 MB)
  [WARN ] coverage   412 prose files on disk, none indexed
               headings and internal links are invisible to `context` / `search_code`
               fix: code-explorer analyze /home/dev/projects/handbook --force --include-docs
  [WARN ] bulk       1 large directory would be indexed for nothing
               src                         5402 files    61.2 MB  5350 parseable - this repository's own source
               vendor                       612 files     4.1 MB  612 parseable - consider --exclude vendor
               node_modules              124068 files     2.2 GB  dropped by 'node_modules'
               fix: code-explorer analyze /home/dev/projects/handbook --force --exclude vendor
  [WARN ] freshness  index is 2 commit(s) behind HEAD
               fix: code-explorer analyze /home/dev/projects/handbook

  Verdict: usable, 3 warning(s).
```

### Reading `bulk`

A directory is *large* at 500 files or 50 MB. `bulk` scans with `.gitignore`
alone, so it sees everything git tracks **and** everything the default
exclusions would spare you, and says which is which:

* `N parseable - consider --exclude X` — nothing stops this directory today, and
  it carries a real parsing cost. It is the first thing to drop when a run is
  too slow. Only these raise the level to `warn`.
* `this repository's own source` — it holds more than half the parseable files,
  so it *is* the project. `doctor` will never offer to exclude it; the knob
  there is `--max-files`, not `--exclude`.
* `walked, barely parsed` — large on disk, but almost nothing in it is code
  (a `docs/` tree, a fixture dump). It costs the walk, not the parser.
* `dropped by '<pattern>'` — already handled, named so that "why is
  `node_modules` missing from my graph" has an answer before it is a question.

A directory `.gitignore` covers never appears at all: it was never walked. See
[indexing exclusions](indexing-exclusions.md) for the pattern list.

## `--json`

```json
{
  "requestedPath": ".",
  "canonicalPath": "/home/dev/projects/handbook",
  "checks": [
    {
      "id": "freshness",
      "level": "warn",
      "summary": "index is 2 commit(s) behind HEAD",
      "details": ["indexed 8e6c503f → HEAD 5a41982d"],
      "fix": "code-explorer analyze /home/dev/projects/handbook"
    }
  ],
  "status": "warn"
}
```

`id` and `level` are stable: script against those, never against the summary
wording. `status` is the worst level across all checks.

## Common verdicts

- **`index` error, "no index directory"** — the repository was never indexed
  here. Run the `analyze` the check prints.
- **`registry` warning, "not listed"** — the index exists but the registry does
  not know it. MCP tools still fall back to the on-disk index, but `list_repos`
  will not show it. Re-run `analyze` to register it.
- **`registry` error, "points at a different index directory"** — the checkout
  moved, or two spellings of the path were indexed. `analyze --force` settles it.
- **`coverage` warning, "N prose files on disk, none indexed"** — a book or a
  documentation repository indexed as if it were code. See
  [prose repositories](prose-repositories.md).
- **`freshness` warning, "indexed commit is unknown to this checkout"** — the
  branch was switched, rebased, or the commit was pruned. Re-index.
