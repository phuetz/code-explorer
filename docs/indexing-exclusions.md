# What `analyze` walks — and what it refuses to walk

`.gitignore` is enough for a well-kept repository. It is not enough for the two
cases that actually cost an indexing run:

* a legacy checkout where `node_modules/`, `target/` or `dist/` was **never
  added to `.gitignore`** — the walker used to descend into every byte of it;
* a backup directory git tracks on purpose (`_archive/`, `backups/`,
  `*.bak`) — real files, real parsing cost, zero value in a code graph.

So `analyze` applies a **default exclusion list on top of `.gitignore`**, as a
walk filter: an excluded directory is *pruned*, never entered.

## The default list

```
node_modules  dist  build  target  .next  .venv  venv  __pycache__
coverage  .git  _archive  archive  obj  bin  packages  .nuget
backup*  *.bak
```

A pattern matches a **whole path segment**, case-insensitively. `node_modules`
drops `web/node_modules/react/index.js` and leaves `src/node_modules_helper.ts`
alone. `*` is the only wildcard.

`obj`, `bin`, `packages` and `.nuget` were already hard-coded inside the
walker; they now live in the same list as everything else.

## Changing it

```bash
code-explorer analyze .                       # defaults on
code-explorer analyze . --exclude vendor      # defaults + vendor/
code-explorer analyze . --include build       # defaults, but keep build/
code-explorer analyze . --no-default-excludes # only .gitignore
```

* `--exclude PATTERN` — repeatable, stacks on the defaults.
* `--include PATTERN` — repeatable, **wins over every exclusion**, including
  the defaults. This is the escape hatch for a repository whose real source
  lives under `build/` or `dist/`.
* `--no-default-excludes` — drop the built-in list entirely. `--exclude` still
  applies.

## Per-repository configuration

Two files are read, and merged with the defaults.

`.codeexplorer/config` — one pattern per line, `#` comments, `!` re-includes:

```
# vendored PHP libraries, never ours
vendor
# ... but this generated client IS ours
!vendor/our-client
```

`code-explorer.toml` — the pre-existing knob, finally honoured:

```toml
[ingestion]
ignored_dirs = ["legacy", "third_party"]
```

## The budget: counted before, not discovered after

`analyze` counts the job before starting it, and prints what it found:

```
Indexing repository: /home/dev/projects/app
Exclusions:  node_modules, dist, build, target, .next, .venv, venv, __pycache__, +10 more
Candidates:  5993 parseable of 7590 files walked (0.16s)
```

The count is a walk with no file read — it costs milliseconds even on a large
tree. Past `--max-files` (default **50 000**) `analyze` **refuses** rather than
running for an unbounded time, and says what carries the weight:

```
Refusing to index 61234 candidate files: --max-files is 50000.

Largest directories after exclusions:
  vendor                         31200 parseable    38000 files      1.4 GB
  fixtures                       15000 parseable    15500 files     13.4 MB
  src                             5000 parseable     6000 files     28.6 MB

Drop what you do not need indexed:
  code-explorer analyze /repo --exclude vendor --exclude fixtures --exclude src
Or raise the ceiling deliberately:
  code-explorer analyze /repo --max-files 62000
  (--max-files 0 removes the guard entirely.)
```

Nothing is written when a run is refused: there is no half-index to clean up.

### When the weight *is* the repository

A directory holding more than half the candidates is not something to exclude
— it is the project. Telling someone to drop their own `src/` would be worse
than saying nothing, so the refusal says the opposite:

```
Refusing to index 5525 candidate files: --max-files is 1.

Largest directories after exclusions:
  src                             5342 parseable     5394 files     61.1 MB
  e2e                               82 parseable      106 files    648.6 KB
  scripts                           38 parseable      103 files    606.8 KB

'src' alone holds 5342 of the 5525 candidates: that is this repository's own
source, not something to exclude.

Drop what you do not need indexed:
  code-explorer analyze /repo --exclude e2e --exclude scripts
Raise the ceiling — this repository really is that big:
  code-explorer analyze /repo --max-files 6000
  (--max-files 0 removes the guard entirely.)
```

`doctor`'s `bulk` check applies the same rule, and stays `OK` on a repository
whose only large directory is its own source.

## What this does not fix

Excluding directories reduces **how many** files are parsed. It does not make
the parser faster. On a repository whose sources genuinely number in the
thousands, the parsing phase remains the dominant cost — measured on
WorkflowBuilder, `structure` took 155 ms and `parsing` 27.7 minutes of a
30-minute run. The candidate line exists so that cost is visible in the first
second instead of being discovered half an hour later.
