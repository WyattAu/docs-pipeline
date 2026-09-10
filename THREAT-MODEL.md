# Threat Model — docs-pipeline

Reference: STRIDE. Scope: the crate's public API surface (`render_markdown`,
`sanitize_html`, `SyntaxHighlighter`, `render_embed`, `LatexRenderer`, TOC
extraction) as used by a service that renders **untrusted** markdown from
many authors. Trust boundaries: (1) untrusted markdown/HTML text entering
every public function, (2) URLs inside embed candidates and link/image
syntax, (3) the dependency tree (pulldown-cmark, ammonia, tree-sitter,
KaTeX).

## Assets

| ID | Asset | Example |
|----|-------|---------|
| A1 | Integrity of rendered pages (no XSS in output HTML) | `<script>` or `onerror=` survives rendering into a shared docs site |
| A2 | Availability of the renderer | Adversarial markdown (deep nesting, pathological highlight input) hangs the worker |
| A3 | Trustworthiness of embedded third-party content | An attacker-controlled URL rendered as a trusted embed |

## STRIDE Analysis

| # | Threat | Category | Surface | Mitigation | Verifying test |
|---|--------|----------|---------|------------|----------------|
| T1 | XSS via raw HTML, event handlers, `javascript:` URIs | Elevation/Info disclosure | `render_markdown`, `sanitize_html` | Ammonia-based sanitization on the complete HTML string: scripts, iframes, SVG `onload`, event handler attributes, and `javascript:` URIs are stripped; custom/unknown elements removed by default | `test_strips_script_tags`, `test_strips_onerror_attribute`, `test_strips_onload_attribute`, `test_strips_javascript_uri`, `test_strips_svg_xss`, `test_xss_svg_onload_stripped`, `test_mdx_component_stripped_by_default` |
| T2 | Malicious domain rendered as a trusted embed | Spoofing | `is_domain_whitelisted`, `render_embed` | Static whitelist (YouTube, Figma, Gist, CodePen, x.com); embeds render only as sandboxed iframes (`sandbox="allow-scripts allow-same-origin"`, lazy loading) with a published CSP policy | `test_domain_whitelist`, `test_embeds_have_sandbox`, `test_embeds_have_lazy_loading`, `embed_csp_policy` |
| T3 | Renderer hang/crash on adversarial markdown | DoS | `render_markdown`, `SyntaxHighlighter::highlight_or_fallback` | Fuzz target `fuzz_render` asserts no panics on arbitrary input; tree-sitter highlighting falls back to plain code blocks for unknown languages instead of failing | `fuzz_render.rs` (fuzz/), `test_highlight_falls_back_for_unsupported_known_lang`, `test_unsupported_language` |
| T4 | Unbounded input size / nesting depth | DoS | all render functions | **Not mitigated** — no input size or nesting-depth limit exists; pulldown-cmark is allocation-hungry on deeply nested constructs. Documented residual risk: enforce size limits at the HTTP layer | Code review |
| T5 | KaTeX placeholder smuggling (code blocks mangled) | Tampering | `LatexRenderer` | `$`/`$$` placeholder handling explicitly skips `<pre>`/`<code>` content; validation via `validate_latex` rejects malformed equations | `test_validate_latex`, `test_render_simple_latex`, `test_count_embeds_skips_code_blocks` (same code-block discipline) |
| T6 | TOC/anchor injection (heading text → HTML/slug) | Info disclosure | `extract_toc`, `extract_toc_from_html` | Heading text passes through `html_escape` for inline TOC; HTML TOC extraction parses already-sanitized output | `test_html_escape`, `test_extract_toc_slugification`, `test_extract_toc_from_html` |

## Repudiation

Not applicable — stateless render pipeline, no logs, no history.

## Out of Scope

- Serving infrastructure (CSP headers are published via `embed_csp_policy`
  but must be applied by the web server).
- Post-sanitization output mutation by downstream templates (a downstream
  that concatenates raw strings can reintroduce XSS).
- Phishing via *content* (a link to `evil.com` in sanctioned markdown is
  content moderation, not a crate concern).

## Residual Risks

- **R1 (Medium, accepted):** `is_domain_whitelisted` uses substring matching
  (`url.contains(domain)`), so `https://evil.com/?ref=www.youtube.com`
  passes the check. Bounded in practice: the rendered iframe `src` is
  constructed from whitelisted patterns and sandboxed (T2), but the
  `test_domain_whitelist` guarantee is weaker than it reads. Tighten to
  host-suffix parsing when embed sources grow.
- **R2 (Medium, accepted):** No input-size/depth limits (T4); hostile
  documents can consume CPU/memory proportionally to their size. Enforce
  request-body caps upstream.
- **R3 (Low, accepted):** Ammonia defaults evolve; an upstream sanitizer
  regression would silently weaken T1. No pinned sanitizer snapshot test
  beyond the explicit strip tests listed above.
- **R4 (Low, accepted):** `test_max_embeds_enforced` caps embed count per
  document, but the cap is a render-quality bound, not a security one.
