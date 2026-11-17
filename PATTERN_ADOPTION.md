# PATTERN_ADOPTION

Relationship to the WyattAu pattern canon (references/adoption-format.md).
Rows list patterns with code evidence; patterns without code and without a
scheduled plan are omitted (omission gate: repo ADR per PATTERN-CORE).

| Pattern | Status | Evidence |
|---|---|---|
| T13 | adopted | src/lib.rs:3 (pure string-to-string render functions; no network, clock, or filesystem access in the core) |
| P39 | partial | src/latex.rs:22 (exact-match KaTeX render cache keyed on the input string; the pipeline has no LLM calls, so inference memoization does not exist yet) |
| P67 | partial | fuzz/fuzz_targets/fuzz_render.rs:9 (libFuzzer harness asserts panic-free render, extraction, and sanitization on hostile input; raw-byte mutations, no structurally valid generators, no checked-in corpus) |
| P96 | partial | src/markdown.rs:617 (ammonia sanitization on the render-to-output boundary on every path, per REQ-DP-100; not a per-stage internal verifier) |
