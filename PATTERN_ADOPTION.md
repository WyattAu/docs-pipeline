# PATTERN_ADOPTION: docs-pipeline

Relationship to the WyattAu pattern canon
(`github.com/WyattAu/pattern-standards`), validated by
`scripts/adoption-check.py` per `references/adoption-format.md`.

| Pattern | Status | Evidence |
|---|---|---|
| T1 | adopted | `src/markdown.rs` + `src/types.rs` (markdown parsed into typed IR at the boundary) + `src/syntax.rs` (tree-sitter parse trees) |
| P39 | n/a | canon baseline line names this repo; code shows no implementation of P39: no inference or embedding calls exist, and the only stage cache is latex rendering (`src/latex.rs:22`), which is not inference memoization |

Canon gap noted for the maintainer: `src/sanitize.rs` implements output
hardening (HTML sanitization) for which the catalog has no pattern row
(closest: P116, which covers file uploads; a future output-hardening
pattern could absorb it).
