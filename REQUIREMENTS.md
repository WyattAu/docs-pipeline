# Requirements — docs-pipeline

Numbered, testable requirements. Every requirement maps to at least one named
test or doc-comment contract; security-relevant items cite THREAT-MODEL.md rows.

Scope: Documentation pipeline — markdown rendering with sanitization, syntax highlighting, and typed document model

## Functional

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-DP-001 | Markdown renders to HTML via pulldown-cmark then passes ammonia sanitization; unknown/custom tags are removed by default | MUST |
| REQ-DP-002 | Syntax highlighting is feature-gated per language (`lang-*`); base build compiles without any highlighter | MUST |
| REQ-DP-003 | The document model roundtrips (parse → model → render) for supported inputs | MUST |

## Security

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-DP-100 | Hostile markdown cannot inject scripts: sanitizer runs after rendering on every path (XSS defense) | MUST |
| REQ-DP-101 | Highlighter input is bounded; pathological code blocks cannot exhaust memory (size caps) | SHOULD |

## Observability & API hygiene

| ID | Requirement | Priority |
|----|-------------|----------|
| REQ-DP-900 | All fallible public APIs return typed errors; production `unwrap`/`expect` is denied or explicitly justified with an invariant comment | MUST |
| REQ-DP-901 | Public items carry doc comments with runnable examples where practical | SHOULD |

Reviewed: 2026-09-11
