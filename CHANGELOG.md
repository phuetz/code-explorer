# Changelog

All notable changes to Code Explorer are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.2.0] - 2026-09-09

### Added

- **`code-explorer doctor <path>`** — one pass over an index reporting path and
  worktree identity, index presence and loadability, schema version, registry
  coherence, file coverage per extension and freshness against `HEAD`, each with
  the command that repairs it. Text output and `--json`; exit code 1 when the
  index is unusable. See [docs/doctor.md](docs/doctor.md).
- **Prose indexing** — Markdown, plain text and reStructuredText files become
  indexed documents whose headings are `Section` symbols and whose internal
  links are graph edges, so `context`, `search_code`, `query` and `impact`
  answer on books, documentation sites and knowledge bases. On by default when
  a repository is less than 30 % code files; `analyze --include-docs` /
  `--no-docs` override. See
  [docs/prose-repositories.md](docs/prose-repositories.md).
- **Index schema stamp** — `meta.json` records the index layout version
  (`repo_manager::INDEX_SCHEMA_VERSION`), so an index written by an older or a
  newer build can be recognised instead of silently misread.
- MCP tool errors carry a structured `data` payload: a stable `code`
  (`REPO_NOT_FOUND`, `INVALID_ARGUMENTS`, `WRITE_QUERY_REJECTED`, …), the
  message and a `hint` telling the caller what to do.
- `IncrementalResult` reports `documents_updated` and `folders_pruned`.

### Fixed

- **The MCP server no longer answers "Repository not found" for a repository
  indexed during the session.** The registry was read once at start-up and
  every tool resolved from that snapshot, so an `analyze` run while an editor
  or agent session was open stayed invisible — while the CLI, which never
  consults the registry, answered correctly on the very same index. The server
  now re-reads the registry when the file changes (one `stat` per call, no
  restart), resolves canonical paths (`.`/`..`, trailing separators, symlinked
  parents, relative arguments) and falls back to an index found on disk. The
  residual error names the path searched, the registry consulted, whether an
  index exists there, and the exact `code-explorer analyze` command.
- **Incremental updates no longer leave orphan nodes.** A deletion or a rename
  left the emptied `Folder` nodes behind, and prose documents were not tracked
  at all, so a removed Markdown file kept its `File` and `Section` nodes and an
  edited one kept its old headings. Folders are pruned bottom-up and documents
  take part in the incremental manifest — only when the index already holds
  documents, so watching a code repository does not turn it into a prose one.
- `list_repos` no longer offers repositories whose index has been deleted; the
  hidden count and the repair command are reported in `_meta`.

### Documentation

- [docs/doctor.md](docs/doctor.md), [docs/prose-repositories.md](docs/prose-repositories.md)
  and an incremental section in
  [docs/INDEX-FRESHNESS.md](docs/INDEX-FRESHNESS.md).
