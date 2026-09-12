//! Syntax highlighting with tree-sitter.
//!
//! This module provides syntax highlighting using `tree-sitter-highlight`
//! with one grammar per enabled `lang-*` cargo feature. Languages whose
//! feature is disabled report as unsupported at runtime.
//!
//! All grammar crates communicate through the version-agnostic
//! `tree-sitter-language` ABI, so a single `tree-sitter` generation (0.25)
//! works for every bundled language.

use crate::error::{Error, Result};
use crate::types::{Language, SyntaxTheme};
use regex::Regex;
use std::sync::{LazyLock, OnceLock};
use tracing::debug;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, Highlighter, HtmlRenderer};

/// Global highlight configurations cache
static HIGHLIGHT_CONFIGS: OnceLock<std::collections::HashMap<Language, HighlightConfiguration>> =
    OnceLock::new();

/// Markdown has a split grammar (block + inline); the inline configuration is
/// resolved via the injection callback when the block grammar injects
/// `markdown-inline`.
#[cfg(feature = "lang-markdown")]
static MARKDOWN_INLINE_CONFIG: OnceLock<Option<HighlightConfiguration>> = OnceLock::new();

/// Syntax highlighter for highlighting code
pub struct SyntaxHighlighter {
    /// Current theme
    theme: SyntaxTheme,
    /// Cached highlight configurations
    configs: &'static std::collections::HashMap<Language, HighlightConfiguration>,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter
    pub fn new() -> Self {
        Self::with_theme(SyntaxTheme::default())
    }

    /// Create a new syntax highlighter with a specific theme
    pub fn with_theme(theme: SyntaxTheme) -> Self {
        let configs = HIGHLIGHT_CONFIGS.get_or_init(Self::init_configs);
        Self { theme, configs }
    }

    /// Initialize all highlight configurations
    fn init_configs() -> std::collections::HashMap<Language, HighlightConfiguration> {
        // `mut` is only exercised when at least one `lang-*` feature is on.
        #[allow(unused_mut)]
        let mut configs = std::collections::HashMap::new();

        #[cfg(feature = "lang-rust")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Rust, config);
        }

        #[cfg(feature = "lang-python")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Python, config);
        }

        #[cfg(feature = "lang-javascript")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::JavaScript, config);
        }

        #[cfg(feature = "lang-typescript")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::TypeScript, config);
        }

        #[cfg(feature = "lang-json")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Json, config);
        }

        #[cfg(feature = "lang-toml")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Toml, config);
        }

        #[cfg(feature = "lang-yaml")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Yaml, config);
        }

        #[cfg(feature = "lang-html")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_html::LANGUAGE.into(),
            "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Html, config);
        }

        #[cfg(feature = "lang-css")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_css::LANGUAGE.into(),
            "css",
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Css, config);
        }

        #[cfg(feature = "lang-bash")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Bash, config);
        }

        #[cfg(feature = "lang-markdown")]
        if let Ok(mut config) = HighlightConfiguration::new(
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            tree_sitter_md::INJECTION_QUERY_BLOCK,
            "",
        ) {
            config.configure(THEME_HIGHLIGHT_NAMES);
            configs.insert(Language::Markdown, config);
        }

        // The markdown block grammar injects `markdown-inline` for inline
        // content; register the inline configuration for the injection
        // callback below.
        #[cfg(feature = "lang-markdown")]
        {
            let _ = MARKDOWN_INLINE_CONFIG.get_or_init(|| {
                HighlightConfiguration::new(
                    tree_sitter_md::INLINE_LANGUAGE.into(),
                    "markdown-inline",
                    tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                    tree_sitter_md::INJECTION_QUERY_INLINE,
                    "",
                )
                .map(|mut c| {
                    c.configure(THEME_HIGHLIGHT_NAMES);
                    c
                })
                .ok()
            });
        }

        // SQL is a known language name but has no bundled grammar.
        // Note: SQL highlighting requires an external grammar; falls back to
        // plain output.

        debug!(
            "Initialized {} syntax highlight configurations",
            configs.len()
        );
        configs
    }

    /// Resolve a highlight configuration for an injected language name.
    fn injected_config(&self, lang_name: &str) -> Option<&HighlightConfiguration> {
        #[cfg(feature = "lang-markdown")]
        if lang_name == "markdown-inline" || lang_name == "markdown_inline" {
            return MARKDOWN_INLINE_CONFIG.get_or_init(|| None).as_ref();
        }
        Language::from_name(lang_name).and_then(|l| self.configs.get(&l))
    }

    /// Highlight code and return HTML
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedLanguage`] when the language name is not
    /// recognized or its `lang-*` feature is disabled.
    pub fn highlight(&self, code: &str, language: &str) -> Result<String> {
        let lang =
            Language::from_name(language).ok_or_else(|| Error::unsupported_language(language))?;

        self.highlight_with_lang(code, &lang)
    }

    /// Highlight code with a known language
    pub fn highlight_with_lang(&self, code: &str, language: &Language) -> Result<String> {
        let config = self
            .configs
            .get(language)
            .ok_or_else(|| Error::unsupported_language(language.as_str()))?;

        let mut highlighter = Highlighter::new();
        let highlights = highlighter
            .highlight(config, code.as_bytes(), None, |lang_name| {
                self.injected_config(lang_name)
            })
            .map_err(|e| Error::syntax_highlight(e.to_string()))?;

        let mut renderer = HtmlRenderer::new();
        renderer
            .render(highlights, code.as_bytes(), &|highlight, buf| {
                buf.extend(self.get_css_class(highlight).as_bytes());
            })
            .map_err(|e| Error::syntax_highlight(e.to_string()))?;

        let mut html = String::new();
        html.push_str("<pre class=\"syntax-highlight\"><code>");
        for line in renderer.lines() {
            html.push_str(&html_escape(line));
        }
        html.push_str("</code></pre>");

        Ok(html)
    }

    /// Highlight code, falling back to a plain (escaped) `<pre><code>` block
    /// when the language is unknown or unsupported.
    pub fn highlight_or_fallback(&self, code: &str, language: &str) -> String {
        self.highlight(code, language)
            .unwrap_or_else(|_| format!("<pre><code>{}</code></pre>", html_escape(code)))
    }

    /// Get CSS class for a highlight
    fn get_css_class(&self, highlight: Highlight) -> &'static str {
        let idx = highlight.0;
        if idx < THEME_HIGHLIGHT_NAMES.len() {
            THEME_HIGHLIGHT_NAMES[idx]
        } else {
            ""
        }
    }

    /// Get the current theme
    pub fn theme(&self) -> SyntaxTheme {
        self.theme
    }

    /// Set the theme
    pub fn set_theme(&mut self, theme: SyntaxTheme) {
        self.theme = theme;
    }

    /// Check if a language is supported
    pub fn is_language_supported(&self, language: &str) -> bool {
        Language::from_name(language)
            .map(|lang| self.configs.contains_key(&lang))
            .unwrap_or(false)
    }

    /// Get list of supported languages
    pub fn supported_languages(&self) -> Vec<&'static str> {
        self.configs.keys().map(|lang| lang.as_str()).collect()
    }

    /// Generate CSS stylesheet for the current theme
    pub fn generate_stylesheet(&self) -> String {
        let theme_colors = match self.theme {
            SyntaxTheme::Light => &LIGHT_THEME_COLORS,
            SyntaxTheme::Dark => &DARK_THEME_COLORS,
            SyntaxTheme::HighContrast => &HIGH_CONTRAST_THEME_COLORS,
            SyntaxTheme::Custom => &DARK_THEME_COLORS, // Default to dark for custom
        };

        let mut css = String::from(".syntax-highlight {\n");
        css.push_str("  font-family: 'Fira Code', 'Consolas', monospace;\n");
        css.push_str("  line-height: 1.5;\n");
        css.push_str("  overflow-x: auto;\n");
        css.push_str("  padding: 1em;\n");
        css.push_str("  border-radius: 4px;\n");
        css.push_str(&format!(
            "  background-color: {};\n",
            theme_colors.background
        ));
        css.push_str(&format!("  color: {};\n", theme_colors.foreground));
        css.push_str("}\n\n");

        for (i, name) in THEME_HIGHLIGHT_NAMES.iter().enumerate() {
            if let Some(color) = theme_colors.highlights.get(i) {
                css.push_str(&format!(
                    ".syntax-highlight .{} {{ color: {}; }}\n",
                    name, color
                ));
            }
        }

        css
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Rendered-HTML code block highlighting
// ============================================================================

/// INVARIANT: every pattern passed here is a compile-time constant that was
/// validated at development time. `Regex::new` can only fail on invalid
/// syntax, so a panic from `expect` indicates a bug in this crate's own
/// patterns — never a recoverable runtime condition.
#[allow(clippy::expect_used)]
fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("validated static regex pattern")
}

static CODE_BLOCK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"<pre([^>]*)>\s*<code([^>]*)>([\s\S]*?)</code>\s*</pre>"#));

static LANGUAGE_CLASS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"class="language-([^"]*)""#));

static CODE_CONTENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"<code[^>]*>([\s\S]*?)</code>"#));

/// Highlight all `<pre><code class="language-...">` blocks in rendered HTML.
///
/// Each recognized block is replaced with
/// `<div class="code-block-wrapper"><pre class="syntax-highlight" data-language="..." data-theme="...">…</pre>`
/// plus a copy-to-clipboard button, matching the docs-site rendering pipeline.
/// The theme is stamped on the output as `data-theme` so downstream CSS can
/// restyle per theme. Blocks with unknown or unsupported languages are left
/// untouched.
pub fn highlight_code_blocks(html: &str, theme: SyntaxTheme) -> String {
    let highlighter = SyntaxHighlighter::with_theme(theme);

    CODE_BLOCK_REGEX.replace_all(html, |caps: &regex::Captures| {
        let _pre_attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let code_attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let code_html = &caps[3];

        let lang = LANGUAGE_CLASS_REGEX
            .captures(code_attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str());

        let Some(lang) = lang else {
            return caps[0].to_string();
        };

        let raw = html_decode(code_html);

        match highlighter.highlight(&raw, lang) {
            Ok(highlighted) => {
                format!(
                    r#"<div class="code-block-wrapper"><pre class="syntax-highlight" data-language="{}" data-theme="{}"><code class="language-{}">{}</code></pre><button class="code-copy-btn" onclick="(function(b){{var c=b.parentElement.querySelector('code');navigator.clipboard.writeText(c.textContent).then(function(){{b.textContent='Copied!';setTimeout(function(){{b.textContent='Copy'}},2000)}})}})(this)" aria-label="Copy code to clipboard">Copy</button></div>"#,
                    lang,
                    theme_slug(theme),
                    lang,
                    extract_inner_code(&highlighted)
                )
            }
            Err(_) => caps[0].to_string(),
        }
    })
    .to_string()
}

/// Stable lowercase slug for a theme (used in the `data-theme` attribute).
fn theme_slug(theme: SyntaxTheme) -> &'static str {
    match theme {
        SyntaxTheme::Light => "light",
        SyntaxTheme::Dark => "dark",
        SyntaxTheme::HighContrast => "high-contrast",
        SyntaxTheme::Custom => "custom",
    }
}

fn extract_inner_code(html: &str) -> String {
    CODE_CONTENT_REGEX
        .captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| html.to_string())
}

fn html_decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
}

/// HTML escape helper
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Theme highlight names (standard tree-sitter highlight names)
const THEME_HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant",
    "function.builtin",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.escape",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "comment",
    "constructor",
    "embedded",
    "label",
    "number",
    "repeat",
    "character",
    "conditional",
    "define",
    "include",
    "boolean",
];

/// Theme color definitions
struct ThemeColors {
    background: &'static str,
    foreground: &'static str,
    highlights: &'static [&'static str],
}

/// Dark theme colors (similar to One Dark)
const DARK_THEME_COLORS: ThemeColors = ThemeColors {
    background: "#282c34",
    foreground: "#abb2bf",
    highlights: &[
        "#e06c75", // attribute
        "#e5c07b", // constant
        "#e5c07b", // function.builtin
        "#61afef", // function
        "#c678dd", // keyword
        "#56b6c2", // operator
        "#e06c75", // property
        "#abb2bf", // punctuation
        "#abb2bf", // punctuation.bracket
        "#abb2bf", // punctuation.delimiter
        "#98c379", // string
        "#56b6c2", // string.escape
        "#56b6c2", // string.special
        "#e06c75", // tag
        "#e5c07b", // type
        "#e5c07b", // type.builtin
        "#e06c75", // variable
        "#e5c07b", // variable.builtin
        "#e06c75", // variable.parameter
        "#5c6370", // comment
        "#e5c07b", // constructor
        "#98c379", // embedded
        "#c678dd", // label
        "#d19a66", // number
        "#c678dd", // repeat
        "#98c379", // character
        "#c678dd", // conditional
        "#c678dd", // define
        "#c678dd", // include
        "#d19a66", // boolean
    ],
};

/// Light theme colors (similar to One Light)
const LIGHT_THEME_COLORS: ThemeColors = ThemeColors {
    background: "#fafafa",
    foreground: "#383a42",
    highlights: &[
        "#e45649", // attribute
        "#986801", // constant
        "#a626a4", // function.builtin
        "#4078f2", // function
        "#a626a4", // keyword
        "#0184bc", // operator
        "#e45649", // property
        "#383a42", // punctuation
        "#383a42", // punctuation.bracket
        "#383a42", // punctuation.delimiter
        "#50a14f", // string
        "#0184bc", // string.escape
        "#0184bc", // string.special
        "#e45649", // tag
        "#986801", // type
        "#c18401", // type.builtin
        "#e45649", // variable
        "#986801", // variable.builtin
        "#e45649", // variable.parameter
        "#a0a1a7", // comment
        "#986801", // constructor
        "#50a14f", // embedded
        "#a626a4", // label
        "#986801", // number
        "#a626a4", // repeat
        "#50a14f", // character
        "#a626a4", // conditional
        "#a626a4", // define
        "#a626a4", // include
        "#986801", // boolean
    ],
};

/// High contrast theme colors
const HIGH_CONTRAST_THEME_COLORS: ThemeColors = ThemeColors {
    background: "#000000",
    foreground: "#ffffff",
    highlights: &[
        "#ff6b6b", // attribute
        "#ffd93d", // constant
        "#ffd93d", // function.builtin
        "#6bcfff", // function
        "#ff79c6", // keyword
        "#8be9fd", // operator
        "#ff6b6b", // property
        "#ffffff", // punctuation
        "#ffffff", // punctuation.bracket
        "#ffffff", // punctuation.delimiter
        "#50fa7b", // string
        "#8be9fd", // string.escape
        "#8be9fd", // string.special
        "#ff6b6b", // tag
        "#ffd93d", // type
        "#ffd93d", // type.builtin
        "#ff6b6b", // variable
        "#ffd93d", // variable.builtin
        "#ff6b6b", // variable.parameter
        "#bfbfbf", // comment
        "#ffd93d", // constructor
        "#50fa7b", // embedded
        "#ff79c6", // label
        "#ffb86c", // number
        "#ff79c6", // repeat
        "#50fa7b", // character
        "#ff79c6", // conditional
        "#ff79c6", // define
        "#ff79c6", // include
        "#ffb86c", // boolean
    ],
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_highlighter_creation() {
        let highlighter = SyntaxHighlighter::new();
        assert_eq!(highlighter.theme(), SyntaxTheme::Dark);
    }

    #[test]
    #[cfg(feature = "lang-rust")]
    fn test_rust_highlighting() {
        let highlighter = SyntaxHighlighter::new();
        let code = r#"fn main() { println!("Hello"); }"#;
        let result = highlighter.highlight(code, "rust");

        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("<pre"));
        assert!(html.contains("</pre>"));
        assert!(html.contains("syntax-highlight"));
    }

    #[test]
    #[cfg(feature = "lang-python")]
    fn test_python_highlighting() {
        let highlighter = SyntaxHighlighter::new();
        let code = "def hello():\n    print('Hello')";
        let result = highlighter.highlight(code, "python");

        assert!(result.is_ok());
    }

    #[test]
    #[cfg(feature = "lang-toml")]
    fn test_toml_highlighting() {
        let highlighter = SyntaxHighlighter::new();
        let code = "[package]\nname = \"x\"\nversion = \"0.1.0\"";
        let result = highlighter.highlight(code, "toml");
        assert!(
            result.is_ok(),
            "TOML highlighting must be available (tree-sitter-toml-ng), got: {:?}",
            result.err()
        );
    }

    #[test]
    #[cfg(feature = "lang-markdown")]
    fn test_markdown_highlighting() {
        let highlighter = SyntaxHighlighter::new();
        let code = "# Title\n\nSome *inline* text and `code`.";
        let result = highlighter.highlight(code, "markdown");
        assert!(
            result.is_ok(),
            "Markdown highlighting must be available (tree-sitter-md), got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_unsupported_language() {
        let highlighter = SyntaxHighlighter::new();
        let code = "some code";
        let result = highlighter.highlight(code, "unknown_lang");

        assert!(result.is_err());
    }

    #[test]
    fn test_highlight_falls_back_to_plain_pre_for_unknown_lang() {
        let highlighter = SyntaxHighlighter::new();
        let code = "some <raw> & code";
        let html = highlighter.highlight_or_fallback(code, "unknown_lang");

        assert!(html.starts_with("<pre><code>"));
        assert!(html.ends_with("</code></pre>"));
        assert!(html.contains("&lt;raw&gt;"));
        assert!(html.contains("&amp;"));
        assert!(!html.contains("<raw>"), "raw HTML must be escaped");
    }

    #[test]
    fn test_highlight_falls_back_for_unsupported_known_lang() {
        let highlighter = SyntaxHighlighter::new();
        // SQL is a recognized name but has no bundled grammar.
        assert!(!highlighter.is_language_supported("sql"));
        let html = highlighter.highlight_or_fallback("SELECT 1;", "sql");
        assert!(html.starts_with("<pre><code>SELECT 1;</code></pre>"));
    }

    #[test]
    #[cfg(all(
        feature = "lang-rust",
        feature = "lang-python",
        feature = "lang-javascript",
        feature = "lang-toml"
    ))]
    fn test_is_language_supported() {
        let highlighter = SyntaxHighlighter::new();

        assert!(highlighter.is_language_supported("rust"));
        assert!(highlighter.is_language_supported("python"));
        assert!(highlighter.is_language_supported("js"));
        assert!(highlighter.is_language_supported("toml"));
        assert!(!highlighter.is_language_supported("unknown"));
    }

    #[test]
    fn test_stylesheet_generation() {
        let highlighter = SyntaxHighlighter::new();
        let css = highlighter.generate_stylesheet();

        assert!(css.contains(".syntax-highlight"));
        assert!(css.contains("background-color"));
    }

    #[test]
    fn test_theme_switching() {
        let mut highlighter = SyntaxHighlighter::new();
        assert_eq!(highlighter.theme(), SyntaxTheme::Dark);

        highlighter.set_theme(SyntaxTheme::Light);
        assert_eq!(highlighter.theme(), SyntaxTheme::Light);

        let css = highlighter.generate_stylesheet();
        assert!(css.contains("#fafafa")); // Light theme background
    }

    #[test]
    fn test_html_escape() {
        let escaped = html_escape("<script>alert('xss')</script>");
        assert!(escaped.contains("&lt;"));
        assert!(escaped.contains("&gt;"));
        assert!(!escaped.contains("<script>"));
    }

    // ── highlight_code_blocks (rendered HTML) Tests ─────────────────────

    #[test]
    #[cfg(feature = "lang-rust")]
    fn highlight_rust_code_block() {
        let html =
            r#"<pre><code class="language-rust">fn main() { println!("Hello"); }</code></pre>"#;
        let result = highlight_code_blocks(html, SyntaxTheme::Dark);
        assert!(result.contains("syntax-highlight"));
        assert!(result.contains("data-language=\"rust\""));
        assert!(result.contains("code-block-wrapper"));
        assert!(
            !result.starts_with(html.trim_start()),
            "block should be replaced"
        );
    }

    #[test]
    fn highlight_preserves_unknown_language() {
        let html = r#"<pre><code class="language-brainfuck">+++[>+++<-]</code></pre>"#;
        let result = highlight_code_blocks(html, SyntaxTheme::Dark);
        assert_eq!(result, html);
    }

    #[test]
    fn highlight_preserves_no_language_block() {
        let html = r#"<pre><code>some plain text</code></pre>"#;
        let result = highlight_code_blocks(html, SyntaxTheme::Dark);
        assert_eq!(result, html);
    }

    #[test]
    fn html_decode_roundtrip() {
        assert_eq!(html_decode("&lt;script&gt;"), "<script>");
        assert_eq!(html_decode("&amp;"), "&");
    }
}
