# Prose repositories — books, documentation, knowledge bases

Code Explorer used to index only files whose extension maps to a supported
programming language. A repository of prose therefore indexed to almost
nothing: a book of three hundred chapters became **three nodes**, and
`context` / `search_code` had nothing to answer with.

Prose is now a first-class ingestion phase.

## What gets indexed

| Extension | Kind |
|---|---|
| `.md`, `.markdown`, `.mdown`, `.mkd`, `.mdx` | Markdown |
| `.txt`, `.text` | plain text |
| `.rst` | reStructuredText |

The prose walker obeys the same rules as the code walker: `.gitignore`, the
build-output and vendor exclusions (`obj/`, `bin/`, `node_modules/`,
`packages/`, `.nuget/`), and the 2 MB per-file ceiling.

For each document:

- one **`File`** node, in the same folder tree as source files;
- one **`Section`** node per heading, carrying the heading text, its depth
  (`h1`…`h6`) and its line range — the section runs to the next heading of
  equal or higher rank. ATX (`# Title`), Setext (`Title` over `===` / `---`)
  and reStructuredText underlines are recognised; a `#` inside a fenced code
  block is not a heading, and neither is `#nospace`;
- one **`File --Imports--> File`** edge per resolved internal link, with reason
  `markdown-link`. `impact`, `coupling` and `trace-files` traverse them like
  any other file dependency.

Link resolution handles relative paths, `..`, `/`-rooted paths, anchors and
query strings, and implicit targets (`x` → `x.md`, `x/README.md`,
`x/index.md`). Images (`![alt](src)`), external URLs, `mailto:` and paths
climbing above the repository root are not links to anything in the index and
are ignored. A link to a file that does not exist is counted, not created.

`Section` nodes are in the full-text index, so `search_code` and `query` reach
document headings, and `context <heading>` works exactly as it does on a
function.

## When it runs

By default, prose is indexed when the repository is **less than 30 % code
files** — a book with a build script gets its documents, a code project with a
README does not pay for a documentation pass.

```bash
code-explorer analyze .                    # automatic
code-explorer analyze . --include-docs     # always index prose
code-explorer analyze . --no-docs          # never index prose
```

`analyze` and `status` print the document count; `doctor` reports it in the
`coverage` check and tells you when prose is sitting unindexed.

## Cost

Measured on a synthetic corpus of 300 chapters plus a README and one build
script (release build, `scripts/d2-prose-bench.sh`):

| | Time | Nodes | Edges | Index on disk |
|---|---|---|---|---|
| `--no-docs` | 51 ms | 3 | 2 | 52 KiB |
| default (prose detected) | 69 ms | 1 206 | 2 104 | 1 604 KiB |

301 documents, 901 headings, 301 resolved internal links, for +18 ms.

## Limits

- Prose has no call graph: sections are related by nesting and by links, not by
  resolved references to code symbols.
- In the `watch` incremental engine, a link from an **unchanged** document to a
  **newly added** one is not created until the next full index — the same
  boundary as call resolution from unchanged files. `analyze --incremental`
  rebuilds the graph and is not affected.
