# Changelog

All notable changes to this project are documented here. Format: [Keep a
Changelog](https://keepachangelog.com/) — versions follow [semver](https://semver.org).

## [Unreleased]

## [0.1.4] - 2026-09-12

### Added
- `tests/config_matrix.rs`: the 11 `lang-*` feature gates behavior-proven
  — each gate pins `is_language_supported` / `highlight` (Ok vs
  `UnsupportedLanguage`) / `highlight_or_fallback` (spans vs plain) and
  `supported_languages` against its `cfg!(feature = ...)` flag, so the
  suite passes under `--all-features` (all gates on) and
  `--no-default-features` (all gates off): toggling any gate flips the
  asserted pipeline behavior. SQL (known name, no bundled grammar)
  documented as always-fallback. Dead-knob sweep: zero new dead knobs.

## [0.1.3] - 2026-09-12

### Fixed
- **Dead config sweep (config-knob behavior matrix):** every public knob now
  observably changes behavior; three dead knobs found and wired.
  - `MarkdownOptions::enable_autolinks` was settable but never read —
    pulldown-cmark has no autolinks flag. Added a GFM-style autolink pass
    over the sanitized HTML: bare `https?://` and `www.` URLs become
    `<a href>` links; code regions, existing links, and tag attributes are
    never rewritten; trailing sentence punctuation stays outside the anchor.
  - `LatexRenderer::opts` was stored but never used — all render paths
    called `katex::render` (i.e. `Opts::default()`). `render`,
    `render_display`, `render_inline`, and `validate` now honor the
    configured options; `LatexDocumentRenderer::with_cache` also actually
    caches now (the document path previously bypassed the cache entirely).

### Changed
- `highlight_code_blocks(html, theme)` stamps the theme as a
  `data-theme="light|dark|high-contrast|custom"` attribute on highlighted
  blocks — the parameter previously had zero effect on output.
  `MarkdownParser` retains its `MarkdownOptions` so render-time knobs stay
  live.

### Added
- `tests/config_matrix.rs`: default-vs-configured behavior assertions for
  all 16 knobs (8 markdown option flags, output format, syntax theme ×2
  surfaces, LaTeX cache/delimiters/opts, document-renderer cache, render
  result builders).

## [0.1.2] - 2026-09-09

### Changed
- Replace production `unwrap()` calls on static `LazyLock<Regex>`
  initializers with a `static_regex()` helper using
  `.expect("validated static regex pattern")`, documenting the
  INVARIANT that these patterns are infallible by construction
  (satisfies `clippy::unwrap_used` under `-D warnings`).

## [0.1.1] - 2026-09-09

### Changed
- Fuzz harnesses for markdown render/highlight/sanitize pipeline and
  repo metadata normalization.

## [0.1.0] - 2026-09-05

### Added
- Initial public release.
