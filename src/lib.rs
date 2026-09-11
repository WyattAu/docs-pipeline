//! # docs-pipeline
//!
//! Markdown rendering and syntax highlighting pipeline for documentation
//! sites, extracted from the Tachyon knowledge base renderer.
//!
//! ## Modules
//!
//! - [`markdown`] — markdown → HTML/plain-text/AST rendering with GFM,
//!   wikilinks, admonitions, embeds, block references, TOC extraction, and
//!   ammonia-based sanitization.
//! - [`syntax`] — tree-sitter based syntax highlighting with per-language
//!   cargo features (`lang-rust`, `lang-python`, …), themeable CSS output,
//!   and rendered-HTML code block highlighting.
//! - [`latex`] — KaTeX equation rendering with `$`/`$$` placeholder handling
//!   that skips `<pre>`/`<code>` content.
//! - [`embeds`] — whitelisted rich-media embed rendering (YouTube, Figma,
//!   Gist, CodePen, tweets) as sandboxed iframes.
//! - [`sanitize`] — standalone HTML sanitization.
//! - [`types`] — render options, output formats, results, metadata, themes,
//!   and supported languages.
//! - [`error`] — the pipeline error type.
//!
//! ## Example
//!
//! ```
//! use docs_pipeline::{render_markdown, extract_toc};
//!
//! let md = "# Getting Started\n\nSome **bold** text.";
//! let html = render_markdown(md);
//! assert!(html.contains("<strong>bold</strong>"));
//!
//! let toc = extract_toc(md);
//! assert_eq!(toc.len(), 1);
//! assert_eq!(toc[0].text, "Getting Started");
//! ```
//!
//! ## Syntax highlighting example
//!
//! ```
//! # #[cfg(feature = "lang-rust")] {
//! use docs_pipeline::syntax::SyntaxHighlighter;
//!
//! let hl = SyntaxHighlighter::new();
//! assert!(hl.is_language_supported("rust"));
//! let html = hl.highlight_or_fallback("fn main() {}", "rust");
//! assert!(html.contains("syntax-highlight"));
//! # }
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod embeds;
pub mod error;
pub mod latex;
pub mod markdown;
pub mod sanitize;
pub mod syntax;
pub mod types;

// Re-export commonly used items at the crate root.
pub use embeds::{count_embeds, embed_csp_policy, is_domain_whitelisted, render_embed};
pub use error::{Error, Result};
pub use latex::{LatexDocumentRenderer, LatexRenderer};
pub use markdown::{
    extract_inline_toc, extract_toc, extract_toc_from_html, render_markdown, strip_html_tags,
    try_render_markdown, BlockReference, EmbedBlock, HtmlTocEntry, MarkdownParser, TocEntry,
};
pub use sanitize::sanitize_html;
pub use syntax::{highlight_code_blocks, SyntaxHighlighter};
pub use types::{
    Language, MarkdownOptions, OutputFormat, RenderMetadata, RenderOptions, RenderResult,
    RenderStats, SyntaxTheme,
};
