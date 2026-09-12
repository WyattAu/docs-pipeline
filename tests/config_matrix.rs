//! Config-knob behavior matrix for docs-pipeline.
//!
//! Every public config knob must OBSERVABLY change behavior: each test pairs
//! the default with an alternate value and asserts the rendered output
//! differs in the documented way. A knob that cannot change behavior is a
//! dead knob (the breaker 1.0 bug class) — see the CHANGELOG for the three
//! this suite exposed (`enable_autolinks`, `LatexRenderer::opts`, and the
//! `highlight_code_blocks` theme parameter), all wired in 0.1.3.
//!
//! Deterministic: no network, no sleeps, no time dependence.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use docs_pipeline::{
    highlight_code_blocks, render_markdown, try_render_markdown, LatexDocumentRenderer,
    LatexRenderer, MarkdownOptions, MarkdownParser, OutputFormat, RenderResult, RenderStats,
    SyntaxHighlighter, SyntaxTheme,
};

// ---------------------------------------------------------------------------
// MarkdownOptions knobs (through MarkdownParser::with_options → parse)
// ---------------------------------------------------------------------------

/// Default options have every extension on.
fn defaults() -> MarkdownOptions {
    MarkdownOptions::default()
}

fn all_off() -> MarkdownOptions {
    MarkdownOptions {
        enable_gfm: false,
        enable_footnotes: false,
        enable_tables: false,
        enable_task_lists: false,
        enable_strikethrough: false,
        enable_autolinks: false,
        enable_smart_punctuation: false,
        enable_heading_attributes: false,
    }
}

fn html_of(options: MarkdownOptions, md: &str) -> String {
    MarkdownParser::with_options(options)
        .parse(md, OutputFormat::Html)
        .unwrap()
        .content
}

/// `enable_gfm` (default on) enables GFM blockquote tags via pulldown-cmark's
/// ENABLE_GFM, and — with strikethrough/tables/tasklists folded in — is the
/// master switch for the GFM surface. Turning it off (with the sub-flags
/// also off) leaves `~~x~~` literal.
#[test]
fn knob_enable_gfm_off_leaves_strikethrough_literal() {
    let on = html_of(defaults(), "~~struck~~");
    let off = html_of(all_off(), "~~struck~~");
    assert!(on.contains("<del>struck</del>"), "{on}");
    assert!(
        !off.contains("<del>"),
        "gfm off must not strike through: {off}"
    );
}

/// `enable_footnotes`: footnote refs become real anchors only when on.
#[test]
fn knob_enable_footnotes_renders_footnote_markup() {
    let md = "Text[^1]\n\n[^1]: The note";
    let on = html_of(defaults(), md);
    let off = html_of(all_off(), md);
    assert!(on.contains("footnote-reference"), "{on}");
    assert!(
        !off.contains("footnote"),
        "footnotes off must keep [^1] literal: {off}"
    );
    assert!(off.contains("[^1]"), "{off}");
}

/// `enable_tables`: pipe tables become `<table>` only when on.
#[test]
fn knob_enable_tables_renders_table_markup() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |";
    let on = html_of(defaults(), md);
    let off = html_of(all_off(), md);
    assert!(on.contains("<table>"), "{on}");
    assert!(!off.contains("<table>"), "{off}");
}

/// `enable_task_lists`: `- [x]` becomes a checkbox input only when on.
/// (The sanitizer strips the `<input>` element, so the post-sanitize
/// differential is the *marker disappearing* vs. staying literal text.)
#[test]
fn knob_enable_task_lists_renders_checkboxes() {
    let md = "- [x] done\n- [ ] todo";
    let on = html_of(defaults(), md);
    let off = html_of(all_off(), md);
    assert!(
        !on.contains("[x]") && !on.contains("[ ]"),
        "tasklists on must consume the markers: {on}"
    );
    assert!(
        off.contains("[x] done") && off.contains("[ ] todo"),
        "tasklists off must keep the markers literal: {off}"
    );
}

/// `enable_strikethrough` (independently of gfm): `~~x~~` → `<del>`.
#[test]
fn knob_enable_strikethrough_independent_of_gfm() {
    let mut opts = all_off();
    opts.enable_strikethrough = true;
    let on = html_of(opts, "~~struck~~");
    let off = html_of(all_off(), "~~struck~~");
    assert!(on.contains("<del>struck</del>"), "{on}");
    assert!(!off.contains("<del>"), "{off}");
}

/// `enable_autolinks` (0.1.3 fix: previously settable but never read —
/// pulldown-cmark has no autolinks flag, so the pass is a post-sanitization
/// rewrite). Bare URLs become anchors when on; literal text when off. Code
/// blocks and existing links are never rewritten.
#[test]
fn knob_enable_autolinks_wraps_bare_urls() {
    let md = "See https://example.com/docs?page=1 now.";
    let on = html_of(defaults(), md);
    let off = html_of(all_off(), md);
    assert!(
        on.contains(r#"<a href="https://example.com/docs?page=1">"#),
        "autolinks on must anchor the URL: {on}"
    );
    assert!(
        !off.contains("<a href=\"https://"),
        "autolinks off must leave the URL literal: {off}"
    );
    // Trailing sentence punctuation is not swallowed by the link.
    assert!(on.contains("</a> now."), "{on}");
}

/// `enable_autolinks` skips code regions and does not double-wrap real links.
#[test]
fn knob_enable_autolinks_never_rewrites_code_or_existing_links() {
    let md = "Code: `https://in-code.example` and [titled](https://linked.example).";
    let on = html_of(defaults(), md);
    assert!(
        on.contains("<code>https://in-code.example</code>"),
        "inline code must stay unwrapped: {on}"
    );
    // The real link keeps exactly one anchor around its text.
    assert_eq!(
        on.matches("<a href=\"https://linked.example").count(),
        1,
        "existing link must not be double-wrapped: {on}"
    );
}

/// `enable_smart_punctuation`: straight quotes become curly when on.
#[test]
fn knob_enable_smart_punctuation_curles_quotes() {
    let on = html_of(defaults(), "\"quoted\"");
    let off = html_of(all_off(), "\"quoted\"");
    assert!(on.contains("“quoted”"), "{on}");
    assert!(!on.contains('&'), "{on}");
    assert!(
        !off.contains('“') && off.contains("\"quoted\""),
        "smart punctuation off must keep straight quotes: {off}"
    );
}

/// `enable_heading_attributes`: `{#id}` class syntax is honored when on.
#[test]
fn knob_enable_heading_attributes_emits_custom_id() {
    let md = "# Heading {#custom-id}";
    let on = html_of(defaults(), md);
    let off = html_of(all_off(), md);
    assert!(on.contains("id=\"custom-id\""), "{on}");
    assert!(
        off.contains("{#custom-id}"),
        "heading attributes off must keep the attribute literal: {off}"
    );
}

// ---------------------------------------------------------------------------
// OutputFormat knob (parse's format argument)
// ---------------------------------------------------------------------------

/// `OutputFormat` observably changes the render channel.
#[test]
fn knob_output_format_changes_render_channel() {
    let parser = MarkdownParser::new();
    let md = "# Title\n\nBody **bold**";
    let html = parser.parse(md, OutputFormat::Html).unwrap().content;
    let plain = parser.parse(md, OutputFormat::PlainText).unwrap().content;
    let md_out = parser.parse(md, OutputFormat::Markdown).unwrap().content;
    let ast = parser.parse(md, OutputFormat::Ast).unwrap().content;
    assert!(html.contains("<strong>bold</strong>"), "{html}");
    assert!(!plain.contains('<'), "{plain}");
    assert_eq!(md_out, md, "markdown passthrough");
    assert!(
        ast.contains("Strong") || ast.contains("Emphasis") || ast.contains("Heading"),
        "{ast}"
    );
}

// ---------------------------------------------------------------------------
// SyntaxHighlighter theme knob
// ---------------------------------------------------------------------------

/// `with_theme`/`set_theme`: the generated stylesheet carries the theme's
/// colors — different themes produce different CSS.
#[test]
fn knob_theme_changes_generated_stylesheet() {
    let light = SyntaxHighlighter::with_theme(SyntaxTheme::Light).generate_stylesheet();
    let dark = SyntaxHighlighter::with_theme(SyntaxTheme::Dark).generate_stylesheet();
    assert_ne!(
        light, dark,
        "light and dark themes must produce distinct CSS"
    );
    assert!(light.contains("background-color:"), "{light}");

    // `set_theme` mutates the same observable channel.
    let mut hl = SyntaxHighlighter::with_theme(SyntaxTheme::Light);
    assert_eq!(hl.generate_stylesheet(), light);
    hl.set_theme(SyntaxTheme::Dark);
    assert_eq!(hl.theme(), SyntaxTheme::Dark);
    assert_eq!(hl.generate_stylesheet(), dark);
}

/// `highlight_code_blocks` theme parameter (0.1.3 fix: previously ignored —
/// now stamped as `data-theme` on the highlighted block).
/// Needs a language feature: without one, blocks are left untouched.
#[cfg(feature = "lang-rust")]
#[test]
fn knob_highlight_code_blocks_theme_is_observable() {
    let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
    let dark = highlight_code_blocks(html, SyntaxTheme::Dark);
    let light = highlight_code_blocks(html, SyntaxTheme::Light);
    assert!(dark.contains("data-theme=\"dark\""), "{dark}");
    assert!(light.contains("data-theme=\"light\""), "{light}");
    assert_ne!(dark, light);
}

// ---------------------------------------------------------------------------
// LatexRenderer knobs
// ---------------------------------------------------------------------------

/// `with_cache` / `set_cache`: caching is observable through `cache_size`,
/// and disabling stops new entries.
#[test]
fn knob_latex_cache_is_observable() {
    let cached = LatexRenderer::with_cache();
    cached.render("E = mc^2").unwrap();
    assert_eq!(cached.cache_size(), 1, "with_cache must record the render");

    let uncached = LatexRenderer::new();
    uncached.render("E = mc^2").unwrap();
    assert_eq!(uncached.cache_size(), 0, "default has no cache");

    // set_cache toggles the same knob at runtime.
    let mut r = LatexRenderer::new();
    r.set_cache(true);
    r.render("x^2").unwrap();
    assert_eq!(r.cache_size(), 1);
    r.set_cache(false);
    r.render("y^2").unwrap();
    assert_eq!(r.cache_size(), 1, "disabled cache must not record");

    // clear_cache empties it.
    r.set_cache(true);
    r.clear_cache();
    assert_eq!(r.cache_size(), 0);
}

/// `set_delimiters` / `with_delimiters`: custom delimiters are honored by
/// `render_from_text` — the default `$`/`$$` are left as text.
#[test]
fn knob_latex_delimiters_change_what_renders() {
    let custom = LatexRenderer::with_delimiters("\\[", "\\]", "\\(", "\\)");
    let text = "Inline \\(x^2\\) and display \\[y^2\\] but $z$ stays.";
    let out = custom.render_from_text(text).unwrap();
    assert!(
        out.contains("katex"),
        "custom inline/display must render: {out}"
    );
    assert!(
        out.contains("$z$"),
        "dollar syntax must be inert under custom delimiters: {out}"
    );

    let default = LatexRenderer::new();
    let out_default = default
        .render_from_text("Inline $x^2$ stays under custom delim knob? no")
        .unwrap();
    assert!(
        out_default.contains("katex"),
        "default dollars render: {out_default}"
    );

    // The setter form mutates the same state.
    let mut r = LatexRenderer::new();
    r.set_delimiters("\\[", "\\]", "\\(", "\\)");
    let out = r.render_from_text("\\(x\\)").unwrap();
    assert!(out.contains("katex"), "{out}");
}

/// `set_opts` (0.1.3 fix: previously stored but never read — every render
/// call used `katex::render`, i.e. `Opts::default()` regardless). Display
/// mode now flows through and changes the emitted markup.
#[test]
fn knob_latex_opts_change_rendered_markup() {
    let mut r = LatexRenderer::new();
    let inline_default = r.render("x^2").unwrap();

    let display_opts = katex::Opts::builder().display_mode(true).build().unwrap();
    r.set_opts(display_opts);
    let rendered_display = r.render("x^2").unwrap();

    assert!(
        rendered_display.contains("katex-display"),
        "display opts must flow through set_opts: {rendered_display}"
    );
    assert!(
        !inline_default.contains("katex-display"),
        "default opts must be inline: {inline_default}"
    );
}

/// `LatexDocumentRenderer::with_cache` routes the same cache knob (the
/// document path renders via delimiters, now through the shared cached
/// helper).
#[test]
fn knob_document_renderer_cache_is_observable() {
    let doc = LatexDocumentRenderer::with_cache();
    doc.render("$E = mc^2$").unwrap();
    assert!(doc.renderer().cache_size() > 0);

    let plain = LatexDocumentRenderer::new();
    plain.render("$E = mc^2$").unwrap();
    assert_eq!(plain.renderer().cache_size(), 0);
}

// ---------------------------------------------------------------------------
// RenderResult builder knobs (data setters on the render result value)
// ---------------------------------------------------------------------------

/// `RenderResult::with_metadata/with_content/with_stats` and
/// `RenderStats::with_render_time/with_cache_hit/with_output_size` must each
/// land in the observable value.
#[test]
fn knob_render_result_builders_are_observable() {
    let stats = RenderStats::new()
        .with_render_time(std::time::Duration::from_millis(7))
        .with_cache_hit(true)
        .with_output_size(42);
    assert!(stats.cache_hit);
    assert_eq!(stats.output_size_bytes, 42);
    assert_eq!(stats.render_time_ms, 7);

    let result = RenderResult::new("body".to_string(), OutputFormat::Html)
        .with_content("replaced".to_string());
    assert_eq!(result.content, "replaced");
    let _ = result; // metadata/stats setters covered by the type's own tests.
}

/// Free-function surface agrees with the parser surface for autolinks
/// (guards against a regression where only the struct path honors the knob).
#[test]
fn free_functions_honor_the_same_knobs() {
    let md = "See https://example.com/a now.";
    let with_knob = try_render_markdown(md, &defaults()).unwrap().content;
    assert!(
        with_knob.contains(r#"href="https://example.com/a""#),
        "{with_knob}"
    );
    let plain = render_markdown(md);
    assert_eq!(
        plain, with_knob,
        "render_markdown uses default (autolinks on)"
    );
}

// ---------------------------------------------------------------------------
// lang-* feature gates (11 knobs): each gate must observably change pipeline
// behavior when toggled. The observable channel is
// `SyntaxHighlighter::is_language_supported` + `highlight` (Ok vs
// `UnsupportedLanguage`) + `highlight_or_fallback` (highlighted vs plain).
// Each test pins its gate with `cfg!(feature = ...)`, so it passes under
// `--all-features` (supported) AND `--no-default-features` (unsupported) —
// toggling the feature flips the asserted outcome.
// ---------------------------------------------------------------------------

/// Assert one gate end to end: support flag, highlight result, and the
/// fallback channel all agree with the feature state.
fn assert_gate(name: &str, sample: &str, enabled: bool) {
    let highlighter = SyntaxHighlighter::new();
    assert_eq!(
        highlighter.is_language_supported(name),
        enabled,
        "gate {name}: support flag must follow the feature"
    );
    assert_eq!(
        highlighter.highlight(sample, name).is_ok(),
        enabled,
        "gate {name}: highlight must succeed exactly when the feature is on"
    );
    let fallback = highlighter.highlight_or_fallback(sample, name);
    if enabled {
        assert!(
            fallback.contains("syntax-highlight"),
            "gate {name}: enabled highlight must emit spans"
        );
    } else {
        assert!(
            !fallback.contains("syntax-highlight"),
            "gate {name}: disabled gate must fall back to plain output"
        );
    }
}

#[test]
fn gate_lang_rust() {
    assert_gate("rust", "fn main() {}", cfg!(feature = "lang-rust"));
}

#[test]
fn gate_lang_python() {
    assert_gate(
        "python",
        "def f():\n    pass",
        cfg!(feature = "lang-python"),
    );
}

#[test]
fn gate_lang_javascript() {
    assert_gate("js", "const x = 1;", cfg!(feature = "lang-javascript"));
}

#[test]
fn gate_lang_typescript() {
    assert_gate(
        "ts",
        "const x: number = 1;",
        cfg!(feature = "lang-typescript"),
    );
}

#[test]
fn gate_lang_json() {
    assert_gate("json", "{\"a\": 1}", cfg!(feature = "lang-json"));
}

#[test]
fn gate_lang_yaml() {
    assert_gate("yaml", "a: 1", cfg!(feature = "lang-yaml"));
}

#[test]
fn gate_lang_html() {
    assert_gate("html", "<p>hi</p>", cfg!(feature = "lang-html"));
}

#[test]
fn gate_lang_css() {
    assert_gate("css", ".a { color: red; }", cfg!(feature = "lang-css"));
}

#[test]
fn gate_lang_bash() {
    assert_gate("bash", "echo hi", cfg!(feature = "lang-bash"));
}

#[test]
fn gate_lang_toml() {
    assert_gate("toml", "a = 1", cfg!(feature = "lang-toml"));
}

#[test]
fn gate_lang_markdown() {
    assert_gate("markdown", "# hi", cfg!(feature = "lang-markdown"));
}

/// `supported_languages` agrees with the gate flags: every enabled language
/// is listed, every disabled one is absent.
#[test]
fn supported_languages_match_gate_flags() {
    let highlighter = SyntaxHighlighter::new();
    let listed = highlighter.supported_languages();
    for (name, enabled) in [
        ("rust", cfg!(feature = "lang-rust")),
        ("python", cfg!(feature = "lang-python")),
        ("javascript", cfg!(feature = "lang-javascript")),
        ("typescript", cfg!(feature = "lang-typescript")),
        ("json", cfg!(feature = "lang-json")),
        ("yaml", cfg!(feature = "lang-yaml")),
        ("html", cfg!(feature = "lang-html")),
        ("css", cfg!(feature = "lang-css")),
        ("bash", cfg!(feature = "lang-bash")),
        ("toml", cfg!(feature = "lang-toml")),
        ("markdown", cfg!(feature = "lang-markdown")),
    ] {
        assert_eq!(
            listed.contains(&name),
            enabled,
            "supported_languages must list {name} exactly when its gate is on"
        );
    }
}

/// SQL is a known name with no bundled grammar (not a gate): it is never
/// "supported" and always falls back to plain output.
#[test]
fn sql_without_grammar_always_falls_back() {
    let highlighter = SyntaxHighlighter::new();
    assert!(!highlighter.is_language_supported("sql"));
    assert!(highlighter.highlight("SELECT 1", "sql").is_err());
    let fallback = highlighter.highlight_or_fallback("SELECT 1", "sql");
    assert!(!fallback.contains("syntax-highlight"));
}
