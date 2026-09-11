//! Shared types for the docs pipeline.
//!
//! This module defines render options, output formats, render results,
//! metadata, syntax highlighting themes, and supported languages.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// Markdown parsing options.
///
/// This is the primary options type for [`crate::MarkdownParser`].
pub type RenderOptions = MarkdownOptions;

/// Markdown parsing options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// Enable GitHub Flavored Markdown (GFM)
    pub enable_gfm: bool,

    /// Enable footnotes extension
    pub enable_footnotes: bool,

    /// Enable tables extension
    pub enable_tables: bool,

    /// Enable task lists (checkboxes)
    pub enable_task_lists: bool,

    /// Enable strikethrough text
    pub enable_strikethrough: bool,

    /// Enable autolinks (convert URLs to links)
    pub enable_autolinks: bool,

    /// Enable smart punctuation (quotes, dashes, etc.)
    pub enable_smart_punctuation: bool,

    /// Enable heading attributes
    pub enable_heading_attributes: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            enable_gfm: true,
            enable_footnotes: true,
            enable_tables: true,
            enable_task_lists: true,
            enable_strikethrough: true,
            enable_autolinks: true,
            enable_smart_punctuation: true,
            enable_heading_attributes: true,
        }
    }
}

/// Output format for rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// HTML output
    #[default]
    Html,

    /// Plain text output
    PlainText,

    /// AST (Abstract Syntax Tree) representation
    Ast,

    /// Markdown (pass-through)
    Markdown,
}

/// Render result containing the rendered content and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    /// The rendered content
    pub content: String,

    /// Output format
    pub format: OutputFormat,

    /// Metadata extracted during rendering
    pub metadata: RenderMetadata,

    /// Rendering statistics
    pub stats: RenderStats,
}

impl RenderResult {
    /// Create a new render result
    pub fn new(content: String, format: OutputFormat) -> Self {
        Self {
            content,
            format,
            metadata: RenderMetadata::default(),
            stats: RenderStats::default(),
        }
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: RenderMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set content
    pub fn with_content(mut self, content: String) -> Self {
        self.content = content;
        self
    }

    /// Set statistics
    pub fn with_stats(mut self, stats: RenderStats) -> Self {
        self.stats = stats;
        self
    }
}

/// Metadata extracted during rendering
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderMetadata {
    /// Document title
    pub title: Option<String>,

    /// Document description
    pub description: Option<String>,

    /// Author information
    pub author: Option<String>,

    /// Document tags
    pub tags: Vec<String>,

    /// Custom metadata key-value pairs
    pub custom: BTreeMap<String, String>,

    /// Word count
    pub word_count: usize,

    /// Character count
    pub char_count: usize,

    /// Number of headings
    pub heading_count: usize,

    /// Number of code blocks
    pub code_block_count: usize,
}

impl RenderMetadata {
    /// Create a new render metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tag
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// Set custom metadata
    pub fn set_custom(&mut self, key: String, value: String) {
        self.custom.insert(key, value);
    }

    /// Get custom metadata
    pub fn get_custom(&self, key: &str) -> Option<&String> {
        self.custom.get(key)
    }
}

/// Rendering statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderStats {
    /// Time taken to render in milliseconds
    pub render_time_ms: u64,

    /// Whether cache was used
    pub cache_hit: bool,

    /// Number of LaTeX equations rendered
    pub latex_equations: usize,

    /// Number of code blocks highlighted
    pub code_blocks: usize,

    /// Number of template substitutions
    pub template_substitutions: usize,

    /// Size of rendered output in bytes
    pub output_size_bytes: usize,
}

impl RenderStats {
    /// Create a new render stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Set render time
    pub fn with_render_time(mut self, duration: Duration) -> Self {
        self.render_time_ms = duration.as_millis() as u64;
        self
    }

    /// Set cache hit
    pub fn with_cache_hit(mut self, hit: bool) -> Self {
        self.cache_hit = hit;
        self
    }

    /// Increment LaTeX equation count
    pub fn increment_latex(&mut self) {
        self.latex_equations += 1;
    }

    /// Increment code block count
    pub fn increment_code_blocks(&mut self) {
        self.code_blocks += 1;
    }

    /// Increment template substitution count
    pub fn increment_template_substitutions(&mut self) {
        self.template_substitutions += 1;
    }

    /// Set output size
    pub fn with_output_size(mut self, size: usize) -> Self {
        self.output_size_bytes = size;
        self
    }
}

/// Syntax highlighting theme
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SyntaxTheme {
    /// Light theme
    Light,

    /// Dark theme
    #[default]
    Dark,

    /// High contrast theme
    HighContrast,

    /// Custom theme (user-defined)
    Custom,
}

impl SyntaxTheme {
    /// Map a theme name (e.g. from a site config) to a theme.
    ///
    /// Recognized names: `light`, `one-light`, `github` map to [`SyntaxTheme::Light`];
    /// `high-contrast`, `highcontrast`, `hc` map to [`SyntaxTheme::HighContrast`];
    /// everything else (including `dark`, `one-dark`) maps to [`SyntaxTheme::Dark`].
    pub fn from_theme_name(name: &str) -> Self {
        match name {
            "light" | "one-light" | "github" => SyntaxTheme::Light,
            "high-contrast" | "highcontrast" | "hc" => SyntaxTheme::HighContrast,
            _ => SyntaxTheme::Dark,
        }
    }
}

/// Supported programming languages for syntax highlighting.
///
/// Every variant is listed here regardless of which `lang-*` cargo features
/// are enabled; use [`crate::syntax::SyntaxHighlighter::is_language_supported`] to check
/// availability at runtime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Language {
    /// Rust
    Rust,
    /// Python
    Python,
    /// JavaScript
    JavaScript,
    /// TypeScript
    TypeScript,
    /// JSON
    Json,
    /// TOML
    Toml,
    /// YAML
    Yaml,
    /// HTML
    Html,
    /// CSS
    Css,
    /// SQL (grammar not bundled; falls back to plain output)
    Sql,
    /// Bash / shell
    Bash,
    /// Markdown
    Markdown,
}

impl Language {
    /// Get all supported languages
    pub fn all() -> &'static [Language] {
        &[
            Language::Rust,
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Json,
            Language::Toml,
            Language::Yaml,
            Language::Html,
            Language::Css,
            Language::Sql,
            Language::Bash,
            Language::Markdown,
        ]
    }

    /// Parse language from string
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rust" => Some(Language::Rust),
            "rs" => Some(Language::Rust),
            "python" => Some(Language::Python),
            "py" => Some(Language::Python),
            "javascript" => Some(Language::JavaScript),
            "js" => Some(Language::JavaScript),
            "typescript" => Some(Language::TypeScript),
            "ts" => Some(Language::TypeScript),
            "json" => Some(Language::Json),
            "toml" => Some(Language::Toml),
            "yaml" => Some(Language::Yaml),
            "yml" => Some(Language::Yaml),
            "html" => Some(Language::Html),
            "htm" => Some(Language::Html),
            "css" => Some(Language::Css),
            "sql" => Some(Language::Sql),
            "bash" => Some(Language::Bash),
            "sh" => Some(Language::Bash),
            "shell" => Some(Language::Bash),
            "markdown" | "md" | "markdown-inline" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Get language as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Json => "json",
            Language::Toml => "toml",
            Language::Yaml => "yaml",
            Language::Html => "html",
            Language::Css => "css",
            Language::Sql => "sql",
            Language::Bash => "bash",
            Language::Markdown => "markdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_options_default() {
        let opts = MarkdownOptions::default();
        assert!(opts.enable_gfm);
        assert!(opts.enable_tables);
    }

    #[test]
    fn test_language_from_str() {
        assert_eq!(Language::from_name("rust"), Some(Language::Rust));
        assert_eq!(Language::from_name("py"), Some(Language::Python));
        assert_eq!(
            Language::from_name("markdown-inline"),
            Some(Language::Markdown)
        );
        assert_eq!(Language::from_name("unknown"), None);
    }

    #[test]
    fn test_theme_from_name() {
        assert_eq!(SyntaxTheme::from_theme_name("light"), SyntaxTheme::Light);
        assert_eq!(SyntaxTheme::from_theme_name("github"), SyntaxTheme::Light);
        assert_eq!(
            SyntaxTheme::from_theme_name("high-contrast"),
            SyntaxTheme::HighContrast
        );
        assert_eq!(SyntaxTheme::from_theme_name("dark"), SyntaxTheme::Dark);
        assert_eq!(SyntaxTheme::from_theme_name("nonsense"), SyntaxTheme::Dark);
    }
}
