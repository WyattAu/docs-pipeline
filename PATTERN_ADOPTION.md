# PATTERN_ADOPTION: docs-pipeline

Relationship to the WyattAu pattern canon
(`github.com/WyattAu/pattern-standards`), validated by
`scripts/adoption-check.py` per `references/adoption-format.md`.

| Pattern | Status | Evidence |
|---|---|---|
| T1 | adopted | `src/markdown.rs` + `src/types.rs` (markdown parsed into typed IR at the boundary) + `src/syntax.rs` (tree-sitter parse trees) |

Canon gap noted for the maintainer: `src/sanitize.rs` implements output
hardening (HTML sanitization) for which the catalog has no pattern row
(closest: P116, which covers file uploads; a future output-hardening
pattern could absorb it).
