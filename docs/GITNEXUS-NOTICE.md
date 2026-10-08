# Notice — relationship to GitNexus

Code Explorer is an independent implementation written in Rust on top of tree-sitter. It shares one idea with
[GitNexus](https://github.com/abhigyanpatwari/GitNexus) — a persistent code graph served to AI agents over MCP — but it
contains no GitNexus code: the engine, the data model, the command surface and the language analyzers (14 languages,
including in-depth ASP.NET and legacy .NET support) were written separately.

**Correction.** An earlier version of this notice stated that Code Explorer "started from" GitNexus under Apache-2.0.
That was wrong on both counts: Code Explorer is not a fork, and GitNexus is distributed under PolyForm Noncommercial
1.0.0. No part of GitNexus is used in this repository.

If you find code that you believe comes from GitNexus, please open an issue so it can be checked.
