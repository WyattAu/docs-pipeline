//! LaTeX rendering with KaTeX.
//!
//! This module provides LaTeX equation rendering via the `katex` crate, plus
//! delimiter-based placeholder handling that converts `$...$` (inline) and
//! `$$...$$` (display) regions in already-rendered HTML into KaTeX markup —
//! while skipping content inside `<pre>`/`<code>` blocks so `$` characters in
//! code are never misinterpreted as LaTeX delimiters.

use crate::error::{Error, Result};
use katex::Opts;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, trace};

/// LaTeX renderer for rendering mathematical equations
pub struct LatexRenderer {
    /// KaTeX rendering options
    opts: Opts,

    /// Cache for rendered equations
    cache: Arc<Mutex<HashMap<String, String>>>,

    /// Enable caching
    enable_cache: bool,

    /// Display mode (block) delimiter
    display_delimiter_start: String,

    /// Display mode (block) delimiter end
    display_delimiter_end: String,

    /// Inline mode delimiter
    inline_delimiter_start: String,

    /// Inline mode delimiter end
    inline_delimiter_end: String,
}

impl LatexRenderer {
    /// Create a new LaTeX renderer with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new LaTeX renderer with caching enabled
    pub fn with_cache() -> Self {
        Self {
            enable_cache: true,
            ..Self::default()
        }
    }

    /// Create a new LaTeX renderer with custom delimiters
    pub fn with_delimiters<S1, S2, S3, S4>(
        display_start: S1,
        display_end: S2,
        inline_start: S3,
        inline_end: S4,
    ) -> Self
    where
        S1: Into<String>,
        S2: Into<String>,
        S3: Into<String>,
        S4: Into<String>,
    {
        Self {
            display_delimiter_start: display_start.into(),
            display_delimiter_end: display_end.into(),
            inline_delimiter_start: inline_start.into(),
            inline_delimiter_end: inline_end.into(),
            ..Self::default()
        }
    }

    /// Render through the cache (when enabled) and KaTeX with the
    /// configured options. Every render path routes through here so the
    /// cache knob behaves uniformly across `render`, `render_display`, and
    /// `render_inline` (and therefore across the document renderer).
    fn cached_render(&self, latex: &str, error_label: &str) -> Result<String> {
        if self.enable_cache {
            let cache = self.cache.lock();
            if let Some(cached) = cache.get(latex) {
                trace!("Cache hit for LaTeX");
                return Ok(cached.clone());
            }
        }

        let html = katex::render_with_opts(latex, &self.opts)
            .map_err(|e| Error::latex_render(format!("{}: {}", error_label, e)))?;

        if self.enable_cache {
            let mut cache = self.cache.lock();
            cache.insert(latex.to_string(), html.clone());
        }
        Ok(html)
    }

    /// Render LaTeX to HTML
    pub fn render<S: AsRef<str>>(&self, latex: S) -> Result<String> {
        let latex = latex.as_ref();
        trace!("Rendering LaTeX: {}", latex);
        let html = self.cached_render(latex, "KaTeX error")?;
        debug!("Rendered LaTeX equation");
        Ok(html)
    }

    /// Render display mode LaTeX
    pub fn render_display<S: AsRef<str>>(&self, latex: S) -> Result<String> {
        let latex = latex.as_ref();
        let html = self.cached_render(latex, "KaTeX display mode error")?;
        debug!("Rendered display mode LaTeX");
        Ok(html)
    }

    /// Render inline mode LaTeX
    pub fn render_inline<S: AsRef<str>>(&self, latex: S) -> Result<String> {
        let latex = latex.as_ref();
        let html = self.cached_render(latex, "KaTeX inline mode error")?;
        debug!("Rendered inline mode LaTeX");
        Ok(html)
    }

    /// Render LaTeX from text, finding and replacing LaTeX delimiters.
    ///
    /// Skips content inside `<pre>` and `<code>` tags to avoid interpreting
    /// `$` characters in code blocks as LaTeX delimiters.
    pub fn render_from_text<S: AsRef<str>>(&self, text: S) -> Result<String> {
        let text = text.as_ref();
        let mut result = String::new();
        let mut pos = 0;

        while pos < text.len() {
            let remaining = &text[pos..];

            // Find the nearest special marker
            let next_pre = remaining.find("<pre").map(|p| (p, "pre"));
            let next_code = remaining.find("<code").map(|p| (p, "code"));
            let next_display = remaining
                .find(&self.display_delimiter_start)
                .map(|p| (p, "display"));
            let next_inline = remaining
                .find(&self.inline_delimiter_start)
                .map(|p| (p, "inline"));

            // Pick the earliest match
            let earliest = [next_pre, next_code, next_display, next_inline]
                .into_iter()
                .flatten()
                .min_by_key(|(p, _)| *p);

            let (offset, kind) = match earliest {
                Some(e) => e,
                None => {
                    result.push_str(remaining);
                    break;
                }
            };

            // Copy text before the marker
            result.push_str(&remaining[..offset]);

            match kind {
                "pre" => {
                    let close = "</pre>";
                    if let Some(end) = remaining[offset..].find(close) {
                        let skip_end = offset + end + close.len();
                        result.push_str(&remaining[offset..skip_end]);
                        pos += skip_end;
                    } else {
                        result.push_str(&remaining[offset..]);
                        break;
                    }
                }
                "code" => {
                    let close = "</code>";
                    if let Some(end) = remaining[offset..].find(close) {
                        let skip_end = offset + end + close.len();
                        result.push_str(&remaining[offset..skip_end]);
                        pos += skip_end;
                    } else {
                        result.push_str(&remaining[offset..]);
                        break;
                    }
                }
                "display" => {
                    let after_open = offset + self.display_delimiter_start.len();
                    if let Some(end) = remaining[after_open..].find(&self.display_delimiter_end) {
                        let end = after_open + end;
                        let latex = &remaining[after_open..end];
                        match self.render_display(latex) {
                            Ok(html) => {
                                result.push_str(&html);
                                pos += end + self.display_delimiter_end.len();
                            }
                            Err(_) => {
                                result.push_str(&self.display_delimiter_start);
                                pos += after_open;
                            }
                        }
                    } else {
                        result.push_str(&remaining[offset..offset + 1]);
                        pos += 1;
                    }
                }
                "inline" => {
                    let after_open = offset + self.inline_delimiter_start.len();
                    if let Some(end) = remaining[after_open..].find(&self.inline_delimiter_end) {
                        let end = after_open + end;
                        let latex = &remaining[after_open..end];
                        match self.render_inline(latex) {
                            Ok(html) => {
                                result.push_str(&html);
                                pos += end + self.inline_delimiter_end.len();
                            }
                            Err(_) => {
                                result.push_str(&self.inline_delimiter_start);
                                pos += after_open;
                            }
                        }
                    } else {
                        result.push_str(&remaining[offset..offset + 1]);
                        pos += 1;
                    }
                }
                _ => unreachable!(),
            }
        }

        Ok(result)
    }

    /// Validate LaTeX syntax
    pub fn validate<S: AsRef<str>>(&self, latex: S) -> bool {
        let latex = latex.as_ref();
        katex::render_with_opts(latex, &self.opts).is_ok()
    }

    /// Get rendering options
    pub fn opts(&self) -> &Opts {
        &self.opts
    }

    /// Set rendering options
    ///
    /// Cached renders are keyed on the input only; after swapping options,
    /// call [`Self::clear_cache`] so stale markup is not served.
    pub fn set_opts(&mut self, opts: Opts) {
        self.opts = opts;
    }

    /// Enable or disable caching
    pub fn set_cache(&mut self, enable: bool) {
        self.enable_cache = enable;
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
        debug!("LaTeX cache cleared");
    }

    /// Get cache size
    pub fn cache_size(&self) -> usize {
        self.cache.lock().len()
    }

    /// Set custom delimiters
    pub fn set_delimiters<S1, S2, S3, S4>(
        &mut self,
        display_start: S1,
        display_end: S2,
        inline_start: S3,
        inline_end: S4,
    ) where
        S1: Into<String>,
        S2: Into<String>,
        S3: Into<String>,
        S4: Into<String>,
    {
        self.display_delimiter_start = display_start.into();
        self.display_delimiter_end = display_end.into();
        self.inline_delimiter_start = inline_start.into();
        self.inline_delimiter_end = inline_end.into();
    }
}

impl Default for LatexRenderer {
    fn default() -> Self {
        Self {
            opts: Opts::default(),
            cache: Arc::new(Mutex::new(HashMap::new())),
            enable_cache: false,
            display_delimiter_start: "$$".to_string(),
            display_delimiter_end: "$$".to_string(),
            inline_delimiter_start: "$".to_string(),
            inline_delimiter_end: "$".to_string(),
        }
    }
}

/// LaTeX document renderer for rendering complete LaTeX documents
#[derive(Default)]
pub struct LatexDocumentRenderer {
    /// LaTeX renderer
    renderer: LatexRenderer,
}

impl LatexDocumentRenderer {
    /// Create a new LaTeX document renderer
    pub fn new() -> Self {
        Self {
            renderer: LatexRenderer::new(),
        }
    }

    /// Create a new LaTeX document renderer with caching
    pub fn with_cache() -> Self {
        Self {
            renderer: LatexRenderer::with_cache(),
        }
    }

    /// Render a LaTeX document
    pub fn render<S: AsRef<str>>(&self, latex: S) -> Result<String> {
        self.renderer.render_from_text(latex)
    }

    /// Get the underlying LaTeX renderer
    pub fn renderer(&self) -> &LatexRenderer {
        &self.renderer
    }

    /// Get the LaTeX renderer mutably
    pub fn renderer_mut(&mut self) -> &mut LatexRenderer {
        &mut self.renderer
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_render_simple_latex() {
        let renderer = LatexRenderer::new();
        let result = renderer.render("E = mc^2").unwrap();
        assert!(result.contains("E"));
    }

    #[test]
    fn test_render_with_cache() {
        let renderer = LatexRenderer::with_cache();
        let result1 = renderer.render("E = mc^2").unwrap();
        let result2 = renderer.render("E = mc^2").unwrap();
        assert_eq!(result1, result2);
        assert!(renderer.cache_size() > 0);
    }

    #[test]
    fn test_render_from_text_preserves_content_before_delimiter() {
        let renderer = LatexRenderer::new();
        let text = "Hello world $E = mc^2$ more text";
        let result = renderer.render_from_text(text).unwrap();
        assert!(
            result.contains("Hello world"),
            "Content before $ was dropped"
        );
        assert!(
            result.contains("more text"),
            "Content after closing $ was dropped"
        );
    }

    #[test]
    fn test_render_from_text_dollar_sign_in_code_block() {
        let renderer = LatexRenderer::new();
        let html = "<h2>Docker</h2><pre><code>docker run -d \\\n  -p 8080:8080\n</code></pre><h2>Nginx</h2><pre><code>proxy_set_header Host $host;\nproxy_set_header X-Real-IP $remote_addr;\n</code></pre><h2>TLS</h2>";
        let result = renderer.render_from_text(html).unwrap();
        assert!(
            result.contains("$host"),
            "$host should be preserved in code block"
        );
        assert!(
            result.contains("$remote_addr"),
            "$remote_addr should be preserved in code block"
        );
        assert!(result.contains("Docker"), "Docker section was dropped");
        assert!(result.contains("Nginx"), "Nginx section was dropped");
        assert!(result.contains("TLS"), "TLS section was dropped");
    }

    #[test]
    fn test_render_from_text_display_mode() {
        let renderer = LatexRenderer::new();
        let text = "Before $$x^2$$ after";
        let result = renderer.render_from_text(text).unwrap();
        assert!(result.contains("Before"), "Content before $$ was dropped");
        assert!(result.contains("after"), "Content after $$ was dropped");
    }

    #[test]
    fn test_validate_latex() {
        let renderer = LatexRenderer::new();
        assert!(renderer.validate("E = mc^2"));
        assert!(!renderer.validate("\\invalid latex"));
    }

    #[test]
    fn test_latex_document_renderer() {
        let doc_renderer = LatexDocumentRenderer::new();
        let result = doc_renderer.render("E = mc^2").unwrap();
        assert!(result.contains("E"));
    }
}
