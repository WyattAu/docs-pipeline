//! Markdown parsing and rendering.
//!
//! This module provides markdown parsing capabilities using `pulldown-cmark`,
//! supporting CommonMark and GitHub Flavored Markdown (GFM), plus Tachyon-style
//! extensions:
//!
//! - Wikilinks (`[[target]]`, `[[target|display]]`)
//! - Admonitions (`> [!note]`)
//! - Embeds (`![youtube](id)`, `![{type id]}` extraction)
//! - Block references / transclusions (`![[doc#heading]]`)
//! - Table of contents extraction (from markdown source or rendered HTML)
//!
//! ## Sanitization
//!
//! HTML output is sanitized with `ammonia` before being returned. Script tags,
//! event handlers (`on*`), and `javascript:` URLs are stripped, while `class`
//! and other safe attributes are preserved for syntax highlighting.
//!
//! ## MDX-style component passthrough
//!
//! Raw HTML (including JSX-like components such as `<MyComponent>`) is parsed
//! by `pulldown-cmark` as HTML events and then passed through the ammonia
//! sanitizer. Unknown/custom element tags are **removed** by the default
//! allowlist — consumers who want specific custom components to survive must
//! add them via a custom ammonia builder (see [`crate::sanitize::sanitize_html`]).
//!
//! ## Why no streaming/chunked rendering?
//!
//! pulldown-cmark is already an incremental, event-driven parser (it yields
//! `Event` items via a standard Rust `Iterator`). The `html::push_html` call
//! consumes this stream in a single pass with no intermediate buffering.
//! Benchmark data shows pulldown-cmark renders 1 MB of markdown in <100 ms on
//! modern hardware, so streaming only matters for documents >10 MB — far
//! beyond typical knowledge-base notes. Additionally, the `ammonia` XSS
//! sanitizer operates on the complete HTML string, making true chunked output
//! incorrect (tags can span chunk boundaries). Streaming would add complexity
//! without measurable benefit for this use case.

use crate::embeds;
use crate::error::{Error, Result};
use crate::types::{MarkdownOptions, OutputFormat, RenderMetadata, RenderResult, RenderStats};
use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::cell::Cell;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::{debug, instrument};

// ============================================================================
// Static Regex Patterns (compiled once, zero per-call overhead)
// ============================================================================

/// Compile a static regex pattern.
///
/// INVARIANT: every pattern passed here is a compile-time constant that was
/// validated at development time. `Regex::new` can only fail on invalid
/// syntax, so a panic from `expect` indicates a bug in this crate's own
/// patterns — never a recoverable runtime condition.
#[allow(clippy::expect_used)]
fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("validated static regex pattern")
}

static EMBED_RE: LazyLock<Regex> = LazyLock::new(|| static_regex(r"!\{(\w+):\s*([^}]+)\}"));

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]"));

static ADMONITION_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| static_regex(r"^>\s*\[!(\w+)\]"));

static ADMONITION_BODY_RE: LazyLock<Regex> = LazyLock::new(|| static_regex(r"^>\s?(.*)"));

// Rendered-HTML TOC patterns (used by [`extract_toc_from_html`] and
// [`extract_inline_toc`]; headings must already carry `id` attributes).

static TOC_HEADING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"<h([1-6])[^>]*id="([^"]*)"[^>]*>(.*?)</h[1-6]>"#));

static HTML_STRIP_REGEX: LazyLock<Regex> = LazyLock::new(|| static_regex(r"<[^>]+>"));

static INLINE_TOC_REGEX: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r#"<h([23])[^>]*id="([^"]*)"[^>]*>(.*?)</h[23]>"#));

// ============================================================================
// TOC & Embed Types
// ============================================================================

/// A single entry in the table of contents (extracted from markdown source).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct TocEntry {
    /// Heading level (1-6).
    pub level: usize,
    /// Slugified heading ID for anchor links.
    pub slug: String,
    /// Heading text.
    pub text: String,
}

/// A single entry in a table of contents extracted from **rendered HTML**.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct HtmlTocEntry {
    /// Heading level (1-6).
    pub level: u8,
    /// The heading element's `id` attribute.
    pub id: String,
    /// Heading text (HTML tags stripped).
    pub title: String,
}

/// An embed block extracted from content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct EmbedBlock {
    /// Embed type (youtube, vimeo, figma, mermaid, plantuml, codepen, github).
    pub kind: String,
    /// Embed identifier (video ID, file hash, diagram code, etc).
    pub id: String,
}

/// A block reference (transclusion) parsed from markdown.
/// Syntax: `![[doc-id]]` or `![[doc-id#heading]]` or `![[doc-id#^block-id]]`
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct BlockReference {
    /// The target document slug or ID.
    pub target: String,
    /// Optional heading to transclude within the target document.
    pub heading: Option<String>,
    /// Optional block-level reference (e.g., `^block-id`).
    pub block_id: Option<String>,
    /// Whether this is a "reference only" (`![[doc-id#^block-id]]`) vs. full embed.
    pub reference_only: bool,
}

// ============================================================================
// MarkdownParser
// ============================================================================

/// Markdown parser for parsing and rendering markdown documents
pub struct MarkdownParser {
    /// The options this parser was configured with (retained so every knob
    /// stays observable at render time, not just compile-into-cmark time).
    options: MarkdownOptions,
    /// Compiled pulldown-cmark options
    cmark_options: Options,
}

impl MarkdownParser {
    /// Create a new markdown parser with default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new markdown parser with custom options
    pub fn with_options(options: MarkdownOptions) -> Self {
        let cmark_options = Self::build_cmark_options(&options);
        Self {
            options,
            cmark_options,
        }
    }

    /// Build pulldown-cmark options from our MarkdownOptions
    fn build_cmark_options(opts: &MarkdownOptions) -> Options {
        let mut options = Options::empty();

        if opts.enable_gfm {
            options.insert(Options::ENABLE_STRIKETHROUGH);
            options.insert(Options::ENABLE_TABLES);
            options.insert(Options::ENABLE_TASKLISTS);
        }

        if opts.enable_footnotes {
            options.insert(Options::ENABLE_FOOTNOTES);
        }

        if opts.enable_strikethrough && !opts.enable_gfm {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }

        if opts.enable_tables && !opts.enable_gfm {
            options.insert(Options::ENABLE_TABLES);
        }

        if opts.enable_task_lists && !opts.enable_gfm {
            options.insert(Options::ENABLE_TASKLISTS);
        }

        if opts.enable_smart_punctuation {
            options.insert(Options::ENABLE_SMART_PUNCTUATION);
        }

        if opts.enable_heading_attributes {
            options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        }

        options
    }

    /// Extract table of contents headings from markdown content.
    ///
    /// Returns heading level (1-6), slug, and text for each heading found
    /// outside of code blocks.
    pub fn extract_toc(content: &str) -> Vec<TocEntry> {
        let mut entries = Vec::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }
            let trimmed = line.trim_start();
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            if level == 0 || level > 6 {
                continue;
            }
            let rest = &trimmed[level..];
            // An ATX heading requires a space (or end of line) after the hashes.
            if !rest.is_empty() && !rest.starts_with(' ') {
                continue;
            }
            let text = rest.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let slug = text
                .to_lowercase()
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            entries.push(TocEntry { level, slug, text });
        }
        entries
    }

    /// Extract embed blocks `!{type id}` from content.
    ///
    /// Recognized types: youtube, vimeo, figma, mermaid, plantuml, codepen, github.
    /// Skips content inside code blocks.
    pub fn extract_embeds(content: &str) -> Vec<EmbedBlock> {
        let mut embeds = Vec::new();
        let mut in_code_block = false;

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if !in_code_block {
                for caps in EMBED_RE.captures_iter(line) {
                    let kind = caps[1].to_lowercase();
                    let id = caps[2].trim().to_string();
                    if !id.is_empty() {
                        embeds.push(EmbedBlock { kind, id });
                    }
                }
            }
        }
        embeds
    }

    /// Pre-process admonition blocks `> [!type]` into HTML divs.
    ///
    /// Converts blocks like:
    /// ```markdown
    /// > [!note]
    /// > This is a note
    /// ```
    ///
    /// Into:
    /// ```html
    /// <div class="admonition admonition-note"><div class="admonition-title">Note</div><div class="admonition-content">
    /// This is a note
    /// </div></div>
    /// ```
    fn preprocess_admonitions(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_code_block = false;
        let mut in_admonition = false;
        let mut admonition_type = String::new();
        let mut admonition_lines: Vec<String> = Vec::new();

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                if in_admonition {
                    result.push_str(&format_admonition_html(&admonition_type, &admonition_lines));
                    in_admonition = false;
                    admonition_lines.clear();
                }
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if !in_admonition {
                if let Some(caps) = ADMONITION_HEADER_RE.captures(line) {
                    in_admonition = true;
                    // INVARIANT: group 1 always participates when the pattern matches.
                    #[allow(clippy::expect_used)]
                    {
                        admonition_type = caps
                            .get(1)
                            .expect("capture group 1 always matches")
                            .as_str()
                            .to_lowercase();
                    }
                    continue;
                }
            }

            if in_admonition {
                if let Some(caps) = ADMONITION_BODY_RE.captures(line) {
                    // INVARIANT: group 1 always participates when the pattern matches.
                    #[allow(clippy::expect_used)]
                    {
                        admonition_lines.push(
                            caps.get(1)
                                .expect("capture group 1 always matches")
                                .as_str()
                                .to_string(),
                        );
                    }
                    continue;
                } else {
                    result.push_str(&format_admonition_html(&admonition_type, &admonition_lines));
                    in_admonition = false;
                    admonition_lines.clear();
                }
            }

            result.push_str(line);
            result.push('\n');
        }

        if in_admonition {
            result.push_str(&format_admonition_html(&admonition_type, &admonition_lines));
        }

        if result.ends_with('\n') {
            result.pop();
        }

        result
    }

    /// Pre-process wikilinks [[target]] and [[target|display]] into HTML anchors.
    ///
    /// Converts to `<a href="/documents/{slug}" class="wikilink">{text}</a>`.
    /// Skips wikilinks inside code blocks.
    fn preprocess_wikilinks(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_code_block = false;

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block {
                result.push_str(line);
                result.push('\n');
            } else {
                let replaced = WIKILINK_RE.replace_all(line, |caps: &regex::Captures| {
                    let target: &str = &caps[1];
                    let display: &str = match caps.get(2) {
                        Some(m) => m.as_str(),
                        None => target,
                    };
                    let slug = target
                        .to_lowercase()
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                                c
                            } else {
                                '-'
                            }
                        })
                        .collect::<String>();
                    format!(
                        "<a href=\"/documents/{}\" class=\"wikilink\">{}</a>",
                        slug, display
                    )
                });
                result.push_str(&replaced);
                result.push('\n');
            }
        }

        result.pop();
        result
    }

    /// Pre-process embed blocks `![type](url)` into raw HTML.
    ///
    /// Converts recognized embed types (youtube, figma, gist, codepen, tweet)
    /// into HTML embed markup. Passes through unrecognized image syntax unchanged.
    /// Skips content inside code blocks.
    fn preprocess_embeds(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_code_block = false;

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            let mut processed = line.to_string();
            for alt in ["youtube", "figma", "gist", "codepen", "tweet"] {
                let needle = format!("![{}](", alt);
                while let Some(pos) = processed.find(&needle) {
                    let after = &processed[pos + needle.len()..];
                    if let Some(close) = after.find(')') {
                        let url = after[..close].trim().to_string();
                        if !url.is_empty() {
                            if let Some(html) = embeds::render_embed(alt, &url) {
                                let end = pos + needle.len() + close + 1;
                                processed.replace_range(pos..end, &html);
                                continue;
                            }
                        }
                        break;
                    } else {
                        break;
                    }
                }
            }
            result.push_str(&processed);
            result.push('\n');
        }

        if result.ends_with('\n') {
            result.pop();
        }
        result
    }

    /// Extract block references (transclusions) from markdown content.
    ///
    /// Parses `![[target]]`, `![[target#heading]]`, `![[target#^block-id]]` syntax.
    /// Skips references inside code blocks and inline code.
    ///
    /// Returns the block references found and their positions.
    pub fn extract_block_references(&self, content: &str) -> Vec<(usize, BlockReference)> {
        let mut references = Vec::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        for (line_idx, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                if !in_code_block {
                    in_code_block = true;
                    code_fence_marker = line.trim().chars().take(3).collect();
                } else if line.trim().starts_with(&code_fence_marker) {
                    in_code_block = false;
                    code_fence_marker.clear();
                }
                continue;
            }

            if in_code_block {
                continue;
            }

            let mut search_start = 0;
            let chars: Vec<char> = line.chars().collect();

            while search_start < chars.len() {
                if chars.get(search_start) == Some(&'`') {
                    let tick_count = count_consecutive(&chars[search_start..], '`');
                    let close_pos =
                        find_closing_backtick(&chars, search_start + tick_count, tick_count);
                    if let Some(pos) = close_pos {
                        search_start = pos + tick_count;
                    } else {
                        search_start = chars.len();
                    }
                    continue;
                }

                if search_start + 2 < chars.len()
                    && chars[search_start] == '!'
                    && chars[search_start + 1] == '['
                    && chars[search_start + 2] == '['
                {
                    let close = find_closing_brackets(&chars, search_start + 3, '[', ']');
                    if let Some(end_pos) = close {
                        let inner: String = chars[search_start + 3..end_pos].iter().collect();
                        if let Some(reference) = parse_block_reference(&inner) {
                            let offset = content[..]
                                .lines()
                                .take(line_idx)
                                .map(|l| l.len() + 1)
                                .sum::<usize>()
                                + search_start;
                            references.push((offset, reference));
                        }
                        search_start = end_pos + 2;
                    } else {
                        search_start += 1;
                    }
                } else {
                    search_start += 1;
                }
            }
        }

        references
    }

    /// Extract all wikilink targets from content (without converting)
    pub fn extract_wikilinks(content: &str) -> Vec<String> {
        let mut in_code_block = false;
        let mut targets = Vec::new();

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if !in_code_block {
                for caps in WIKILINK_RE.captures_iter(line) {
                    targets.push(caps[1].to_string());
                }
            }
        }

        targets
    }

    /// Parse markdown content
    #[instrument(skip(self, markdown), fields(format = ?format))]
    pub fn parse<S: AsRef<str>>(&self, markdown: S, format: OutputFormat) -> Result<RenderResult> {
        let markdown = markdown.as_ref();
        let markdown_str = Self::preprocess_wikilinks(markdown);
        let start_time = Instant::now();

        debug!("Parsing markdown content ({} bytes)", markdown_str.len());

        let (content, metadata, stats) = match format {
            OutputFormat::Html => self.parse_to_html(&markdown_str)?,
            OutputFormat::PlainText => self.parse_to_plain_text(&markdown_str)?,
            OutputFormat::Ast => self.parse_to_ast(&markdown_str)?,
            OutputFormat::Markdown => {
                let metadata = self.extract_metadata(&markdown_str);
                let stats = RenderStats::new()
                    .with_render_time(start_time.elapsed())
                    .with_output_size(markdown_str.len());
                (markdown_str.to_string(), metadata, stats)
            }
        };

        let render_time = start_time.elapsed();
        let stats = stats
            .with_render_time(render_time)
            .with_output_size(content.len());

        debug!(
            "Parsed markdown in {}ms, output {} bytes",
            render_time.as_millis(),
            content.len()
        );

        Ok(RenderResult::new(content, format)
            .with_metadata(metadata)
            .with_stats(stats))
    }

    /// Parse markdown to HTML
    fn parse_to_html(&self, markdown: &str) -> Result<(String, RenderMetadata, RenderStats)> {
        let markdown = Self::preprocess_wikilinks(markdown);
        let markdown = Self::preprocess_admonitions(&markdown);
        let markdown = Self::preprocess_embeds(&markdown);
        let parser = Parser::new_ext(&markdown, self.cmark_options);

        let metadata = self.extract_metadata(&markdown);
        let mut stats = RenderStats::new();

        let code_block_count = Cell::new(0u32);
        let parser_with_count = parser.inspect(|event| {
            if matches!(event, Event::Start(Tag::CodeBlock(_))) {
                code_block_count.set(code_block_count.get() + 1);
            }
        });

        let mut html_output = String::with_capacity(markdown.len() * 2);
        html::push_html(&mut html_output, parser_with_count);

        html_output = ammonia::Builder::default()
            .add_tags([
                "img",
                "pre",
                "code",
                "span",
                "div",
                "a",
                "iframe",
                "blockquote",
            ])
            .add_generic_attributes(&["class", "id", "style", "data-video-id", "data-tweet-url"])
            .add_tag_attributes("img", ["src", "alt", "title", "width", "height", "loading"])
            .add_tag_attributes("a", ["href", "title"])
            .add_tag_attributes(
                "iframe",
                [
                    "src",
                    "width",
                    "height",
                    "frameborder",
                    "allowfullscreen",
                    "loading",
                    "sandbox",
                ],
            )
            .clean(&html_output)
            .to_string();

        // GFM-style autolink extension: after sanitization (so the anchors we
        // emit are the final, trusted output) and never inside code blocks,
        // existing links, or tag attributes.
        if self.options.enable_autolinks {
            html_output = autolink_html(&html_output);
        }

        for _ in 0..code_block_count.get() {
            stats.increment_code_blocks();
        }

        Ok((html_output, metadata, stats))
    }

    /// Parse markdown to plain text
    fn parse_to_plain_text(&self, markdown: &str) -> Result<(String, RenderMetadata, RenderStats)> {
        let parser = Parser::new_ext(markdown, self.cmark_options);
        let metadata = self.extract_metadata(markdown);
        let stats = RenderStats::new();

        let mut plain_text = String::with_capacity(markdown.len());

        for event in parser {
            match event {
                Event::Text(text) => {
                    plain_text.push_str(&text);
                }
                Event::Code(code) => {
                    plain_text.push_str(&code);
                }
                Event::SoftBreak | Event::HardBreak => {
                    plain_text.push('\n');
                }
                Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Heading(_)) => {
                    plain_text.push_str("\n\n");
                }
                _ => {}
            }
        }

        // Clean up extra whitespace
        let plain_text = plain_text
            .lines()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        Ok((plain_text, metadata, stats))
    }

    /// Parse markdown to AST representation (JSON)
    fn parse_to_ast(&self, markdown: &str) -> Result<(String, RenderMetadata, RenderStats)> {
        let parser = Parser::new_ext(markdown, self.cmark_options);
        let metadata = self.extract_metadata(markdown);
        let stats = RenderStats::new();

        let mut events = Vec::new();
        for event in parser {
            let event_str = match &event {
                Event::Start(tag) => format!("Start: {:?}", tag),
                Event::End(tag_end) => format!("End: {:?}", tag_end),
                Event::Text(text) => format!("Text: {}", text),
                Event::Code(code) => format!("Code: {}", code),
                Event::Html(html) => format!("Html: {}", html),
                Event::InlineHtml(html) => format!("InlineHtml: {}", html),
                Event::InlineMath(math) => format!("InlineMath: {}", math),
                Event::DisplayMath(math) => format!("DisplayMath: {}", math),
                Event::FootnoteReference(name) => format!("FootnoteReference: {}", name),
                Event::SoftBreak => "SoftBreak".to_string(),
                Event::HardBreak => "HardBreak".to_string(),
                Event::Rule => "Rule".to_string(),
                Event::TaskListMarker(checked) => format!("TaskListMarker: {}", checked),
            };
            events.push(event_str);
        }

        let ast = serde_json::to_string_pretty(&events)
            .map_err(|e| Error::serialization(e.to_string()))?;

        Ok((ast, metadata, stats))
    }

    /// Extract metadata from markdown content
    fn extract_metadata(&self, markdown: &str) -> RenderMetadata {
        let mut metadata = RenderMetadata::new();

        let mut word_count = 0;
        let mut char_count = 0;
        let mut heading_count = 0;
        let mut code_block_count = 0;
        let mut first_heading: Option<String> = None;

        for event in Parser::new_ext(markdown, self.cmark_options) {
            match &event {
                Event::Start(Tag::CodeBlock(_)) => {
                    code_block_count += 1;
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    heading_count += 1;
                    if first_heading.is_none() && *level == HeadingLevel::H1 {
                        // Next text event will be the title
                    }
                }
                Event::Text(text) => {
                    char_count += text.len();
                    word_count += text.split_whitespace().count();

                    // Use first H1 as title
                    if first_heading.is_none() {
                        // Simple heuristic: first text in document is often the title
                        if !text.trim().is_empty() {
                            first_heading = Some(text.chars().take(100).collect());
                        }
                    }
                }
                _ => {}
            }
        }

        metadata.title = first_heading;
        metadata.word_count = word_count;
        metadata.char_count = char_count;
        metadata.heading_count = heading_count;
        metadata.code_block_count = code_block_count;

        metadata
    }
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::with_options(MarkdownOptions::default())
    }
}

// ============================================================================
// Autolink pass (GFM-style, post-sanitization)
// ============================================================================

/// Characters that terminate a bare URL but are usually sentence punctuation
/// when they appear at the end (GFM trims these from the match).
const URL_TRAILING_PUNCT: &[char] = &['.', ',', ';', ':', '!', '?', '\'', '"', '*', '_', '~'];

/// Wrap bare `https?://` and `www.` URLs in text segments of rendered HTML
/// with `<a href>` elements (the GFM autolink extension).
///
/// Regions that must never be rewritten are skipped structurally:
/// - inside `<pre>`/`<code>` (code stays code),
/// - inside `<a>`/`</a>` (existing links are not double-wrapped),
/// - inside tags (protects attribute values such as `href="https://…").
///
/// The input is already HTML-escaped and sanitized, so the matched URL text
/// is inserted verbatim (its escapes are preserved, never double-encoded).
fn autolink_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 64);
    let mut skip_depth = 0usize; // <pre>/<code> nesting
    let mut link_depth = 0usize; // <a> nesting
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        let (text, after) = rest.split_at(lt);
        if skip_depth == 0 && link_depth == 0 {
            autolink_text(text, &mut out);
        } else {
            out.push_str(text);
        }

        let tag_end = after.find('>').map_or(after.len(), |p| p + 1);
        let tag = &after[..tag_end];
        let lower = tag.to_ascii_lowercase();
        if lower.starts_with("<pre") || lower.starts_with("<code") {
            skip_depth += 1;
        } else if lower.starts_with("</pre") || lower.starts_with("</code") {
            skip_depth = skip_depth.saturating_sub(1);
        } else if lower.starts_with("<a ") || lower == "<a>" {
            link_depth += 1;
        } else if lower.starts_with("</a") {
            link_depth = link_depth.saturating_sub(1);
        }
        out.push_str(tag);
        rest = &after[tag_end..];
    }

    if skip_depth == 0 && link_depth == 0 {
        autolink_text(rest, &mut out);
    } else {
        out.push_str(rest);
    }
    out
}

/// Emit `text` into `out`, wrapping bare URLs in anchors.
fn autolink_text(text: &str, out: &mut String) {
    let mut cursor = 0;
    while let Some((start, scheme_end)) = find_bare_url(&text[cursor..]) {
        let abs_start = cursor + start;
        let abs_scheme_end = cursor + scheme_end;

        // Extend the match to the end of the URL (stop at whitespace or any
        // tag boundary), then trim trailing sentence punctuation.
        let bytes = text.as_bytes();
        let mut end = abs_scheme_end;
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() && bytes[end] != b'<' {
            end += 1;
        }
        let mut url = &text[abs_start..end];
        // Trim trailing punctuation; a closing paren only counts when the URL
        // does not also contain an opening paren (GFM balance rule).
        loop {
            let Some(last) = url.chars().last() else {
                break;
            };
            let trailing_punct =
                URL_TRAILING_PUNCT.contains(&last) || (last == ')' && !url.contains('('));
            if trailing_punct {
                url = &url[..url.len() - 1];
            } else {
                break;
            }
        }
        if url.is_empty() || !looks_like_url(url) {
            // Not a usable URL (e.g. bare "www." with no host); emit up to
            // and including the scheme prefix verbatim and resume after it.
            out.push_str(&text[cursor..abs_scheme_end]);
            cursor = abs_scheme_end;
            continue;
        }

        out.push_str(&text[cursor..abs_start]);
        out.push_str("<a href=\"");
        out.push_str(url);
        out.push_str("\">");
        out.push_str(url);
        out.push_str("</a>");
        cursor = abs_start + url.len();
    }
    out.push_str(&text[cursor..]);
}

/// Find the next bare URL start in `text`: a byte offset plus the offset just
/// past the scheme, or `None`.
fn find_bare_url(text: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for scheme in ["https://", "http://"] {
        if let Some(p) = text.find(scheme) {
            let candidate = (p, p + scheme.len());
            best = Some(match best {
                Some(b) if b.0 <= candidate.0 => b,
                _ => candidate,
            });
        }
    }
    if let Some(p) = text.find("www.") {
        let at_boundary = p == 0 || !text.as_bytes()[p - 1].is_ascii_alphanumeric();
        if at_boundary {
            let candidate = (p, p + "www.".len());
            best = Some(match best {
                Some(b) if b.0 <= candidate.0 => b,
                _ => candidate,
            });
        }
    }
    best
}

/// A `www.`-prefixed match must carry a plausible host (`www.x.y`) to count.
fn looks_like_url(url: &str) -> bool {
    if url.starts_with("http://") || url.starts_with("https://") {
        return true;
    }
    // www.example.tld — at least one dot after the leading www.
    url.starts_with("www.") && url["www.".len()..].contains('.')
}

// ============================================================================
// Free functions
// ============================================================================

/// Render markdown to HTML with default options, sanitizing the output.
///
/// This is a convenience wrapper around [`MarkdownParser::parse`] with
/// [`OutputFormat::Html`]. On error, the (escaped) input is wrapped in a
/// `<div class="render-error">` block so callers can always insert the result
/// into a page.
pub fn render_markdown(content: &str) -> String {
    try_render_markdown(content, &MarkdownOptions::default())
        .map(|r| r.content)
        .unwrap_or_else(|_| format!("<div class=\"render-error\">{}</div>", html_escape(content)))
}

/// Render markdown with explicit options, returning the full render result.
pub fn try_render_markdown(content: &str, options: &MarkdownOptions) -> Result<RenderResult> {
    MarkdownParser::with_options(options.clone()).parse(content, OutputFormat::Html)
}

/// Extract table of contents headings from markdown source.
///
/// See [`MarkdownParser::extract_toc`].
pub fn extract_toc(content: &str) -> Vec<TocEntry> {
    MarkdownParser::extract_toc(content)
}

/// Extract table of contents from **rendered HTML**.
///
/// Matches `<h1>`–`<h6>` elements that already carry `id` attributes
/// (e.g. assigned by an `add_heading_ids` pass) and strips inner HTML tags
/// from the titles.
pub fn extract_toc_from_html(html: &str) -> Vec<HtmlTocEntry> {
    TOC_HEADING_REGEX
        .captures_iter(html)
        .map(|cap| HtmlTocEntry {
            level: cap[1].parse().unwrap_or(2),
            id: cap[2].to_string(),
            title: decode_basic_entities(&strip_html_tags(&cap[3])),
        })
        .collect()
}

/// Extract an inline (h2/h3 only) table of contents from rendered HTML.
pub fn extract_inline_toc(html: &str) -> Vec<HtmlTocEntry> {
    INLINE_TOC_REGEX
        .captures_iter(html)
        .map(|cap| HtmlTocEntry {
            level: cap[1].parse().unwrap_or(2),
            id: cap[2].to_string(),
            title: decode_basic_entities(&strip_html_tags(&cap[3])),
        })
        .collect()
}

/// Strip all HTML tags from a string (used for TOC titles).
pub fn strip_html_tags(html: &str) -> String {
    HTML_STRIP_REGEX.replace_all(html, "").to_string()
}

/// Decode the five basic HTML entities in TOC titles so downstream escaping
/// does not double-escape them.
fn decode_basic_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Escape a string for safe inclusion in HTML text content.
#[allow(dead_code)]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Format an admonition block as HTML.
fn format_admonition_html(admonition_type: &str, lines: &[String]) -> String {
    let title = match admonition_type {
        "note" => "Note",
        "tip" => "Tip",
        "info" => "Info",
        "warning" => "Warning",
        "danger" => "Danger",
        "caution" => "Caution",
        _ => admonition_type,
    };

    let content = lines.join("\n");

    format!(
        "<div class=\"admonition admonition-{type}\">\
         <div class=\"admonition-title\">{title}</div>\
         <div class=\"admonition-content\">{content}</div>\
         </div>",
        type = admonition_type,
        title = title,
        content = content
    )
}

/// Parse the inner content of a block reference `[[target]]`, `[[target#heading]]`, `[[target#^block-id]]`.
fn parse_block_reference(inner: &str) -> Option<BlockReference> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }

    let (inner, reference_only) = if let Some(stripped) = inner.strip_prefix('!') {
        (stripped, true)
    } else {
        (inner, false)
    };

    let target;
    let mut heading = None;
    let mut block_id = None;

    if let Some(hash_pos) = inner.find('#') {
        target = inner[..hash_pos].trim().to_string();
        let fragment = &inner[hash_pos + 1..];

        if let Some(stripped) = fragment.strip_prefix('^') {
            block_id = Some(stripped.trim().to_string());
        } else {
            heading = Some(fragment.trim().to_string());
        }
    } else {
        target = inner.trim().to_string();
    }

    if target.is_empty() {
        return None;
    }

    Some(BlockReference {
        target,
        heading,
        block_id,
        reference_only,
    })
}

/// Count consecutive occurrences of a character at the start of a slice.
fn count_consecutive(chars: &[char], target: char) -> usize {
    chars.iter().take_while(|c| **c == target).count()
}

/// Find the closing backtick matching the opening tick count.
fn find_closing_backtick(chars: &[char], start: usize, tick_count: usize) -> Option<usize> {
    let mut pos = start;
    while pos + tick_count <= chars.len() {
        if chars[pos] == '`' && count_consecutive(&chars[pos..], '`') >= tick_count {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

/// Find closing `]]` bracket pair.
fn find_closing_brackets(chars: &[char], start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1i32;
    let mut pos = start;
    while pos < chars.len() {
        if chars[pos] == open {
            depth += 1;
        } else if chars[pos] == close {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_parse_simple_markdown() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("# Hello World\n\nThis is a test.", OutputFormat::Html)
            .unwrap();

        assert!(result.content.contains("<h1>"));
        assert!(result.content.contains("Hello World"));
        assert_eq!(result.format, OutputFormat::Html);
    }

    #[test]
    fn test_parse_to_plain_text() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("# Hello World\n\nThis is a test.", OutputFormat::PlainText)
            .unwrap();

        assert!(result.content.contains("Hello World"));
        assert!(result.content.contains("This is a test"));
        assert!(!result.content.contains("<"));
    }

    #[test]
    fn test_parse_to_ast() {
        let parser = MarkdownParser::new();
        let result = parser.parse("# Hello", OutputFormat::Ast).unwrap();

        assert!(result.content.contains("Start"));
        assert!(result.content.contains("Heading"));
    }

    #[test]
    fn test_metadata_extraction() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("# Document Title\n\nSome content here.", OutputFormat::Html)
            .unwrap();

        assert!(result.metadata.title.is_some());
        assert_eq!(result.metadata.heading_count, 1);
        assert!(result.metadata.word_count > 0);
    }

    #[test]
    fn test_code_block_counting() {
        let parser = MarkdownParser::new();
        let markdown = r#"
# Code Example

```rust
fn main() {
    println!("Hello");
}
```

Some more text.
"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert_eq!(result.metadata.code_block_count, 1);
    }

    #[test]
    fn test_youtube_embed_rendered() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("![youtube](dQw4w9WgXcQ)", OutputFormat::Html)
            .unwrap();
        assert!(result.content.contains("embed-youtube"));
        assert!(result.content.contains("youtube.com/embed/dQw4w9WgXcQ"));
        assert!(result.content.contains("iframe"));
    }

    #[test]
    fn test_figma_embed_rendered() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse(
                "![figma](https://www.figma.com/file/abc)",
                OutputFormat::Html,
            )
            .unwrap();
        assert!(result.content.contains("embed-figma"));
        assert!(result.content.contains("figma.com/embed"));
    }

    #[test]
    fn test_gist_embed_rendered() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse(
                "![gist](https://gist.github.com/user/abc)",
                OutputFormat::Html,
            )
            .unwrap();
        assert!(result.content.contains("embed-gist"));
        assert!(result.content.contains("gist.github.com/user/abc.js"));
    }

    #[test]
    fn test_codepen_embed_rendered() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse(
                "![codepen](https://codepen.io/user/pen/abc)",
                OutputFormat::Html,
            )
            .unwrap();
        assert!(result.content.contains("embed-codepen"));
        assert!(result.content.contains("codepen.io/embed/"));
    }

    #[test]
    fn test_tweet_embed_rendered() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse(
                "![tweet](https://x.com/user/status/123)",
                OutputFormat::Html,
            )
            .unwrap();
        assert!(result.content.contains("embed-tweet"));
        assert!(result.content.contains("twitter-tweet"));
    }

    #[test]
    fn test_embeds_have_lazy_loading() {
        let parser = MarkdownParser::new();
        let result = parser.parse("![youtube](abc)", OutputFormat::Html).unwrap();
        assert!(result.content.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_embeds_have_sandbox() {
        let parser = MarkdownParser::new();
        let result = parser.parse("![youtube](abc)", OutputFormat::Html).unwrap();
        assert!(result.content.contains("sandbox="));
    }

    #[test]
    fn test_embeds_skipped_in_code_blocks() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("```\n![youtube](abc)\n```", OutputFormat::Html)
            .unwrap();
        assert!(!result.content.contains("embed-youtube"));
    }

    #[test]
    fn test_regular_images_preserved() {
        let parser = MarkdownParser::new();
        let result = parser
            .parse("![alt](https://example.com/img.png)", OutputFormat::Html)
            .unwrap();
        assert!(result.content.contains("<img"));
        assert!(result
            .content
            .contains("src=\"https://example.com/img.png\""));
    }

    #[test]
    fn test_gfm_features() {
        let parser = MarkdownParser::new();
        let markdown = "| Col1 | Col2 |\n|------|------|\n| A | B |";
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(result.content.contains("<table"));
    }

    #[test]
    fn test_pass_through() {
        let parser = MarkdownParser::new();
        let markdown = "# Hello";
        let result = parser.parse(markdown, OutputFormat::Markdown).unwrap();

        assert_eq!(result.content, markdown);
    }

    #[test]
    fn test_preprocess_wikilinks_basic() {
        let result = MarkdownParser::preprocess_wikilinks("See [[Hello]] for details");
        assert_eq!(
            result,
            r#"See <a href="/documents/hello" class="wikilink">Hello</a> for details"#
        );
    }

    #[test]
    fn test_preprocess_wikilinks_with_display() {
        let result = MarkdownParser::preprocess_wikilinks("Click [[Hello|Click here]] now");
        assert_eq!(
            result,
            r#"Click <a href="/documents/hello" class="wikilink">Click here</a> now"#
        );
    }

    #[test]
    fn test_preprocess_wikilinks_in_code_block() {
        let input = "Before\n```\n[[Hello]]\n```\nAfter [[World]]";
        let result = MarkdownParser::preprocess_wikilinks(input);
        assert!(
            result.contains("[[Hello]]"),
            "wikilink inside code block should NOT be converted"
        );
        assert!(
            result.contains(r#"<a href="/documents/world" class="wikilink">World</a>"#),
            "wikilink outside code block should be converted to HTML anchor"
        );
    }

    #[test]
    fn test_preprocess_wikilinks_multiple() {
        let input = "Check [[Alpha]], [[Beta|the beta doc]], and [[Gamma]]";
        let result = MarkdownParser::preprocess_wikilinks(input);
        assert_eq!(
            result,
            concat!(
                r#"Check <a href="/documents/alpha" class="wikilink">Alpha</a>, "#,
                r#"<a href="/documents/beta" class="wikilink">the beta doc</a>, "#,
                r#"and <a href="/documents/gamma" class="wikilink">Gamma</a>"#
            )
        );
    }

    #[test]
    fn test_preprocess_wikilinks_slug_with_special_chars() {
        let result = MarkdownParser::preprocess_wikilinks("[[My Document Title]]");
        assert_eq!(
            result,
            r#"<a href="/documents/my-document-title" class="wikilink">My Document Title</a>"#
        );
    }

    #[test]
    fn test_extract_wikilinks() {
        let input = "Link to [[Foo]] and [[Bar|display]]\n```\n[[Ignored]]\n```\n[[After]]";
        let targets = MarkdownParser::extract_wikilinks(input);
        assert_eq!(targets, vec!["Foo", "Bar", "After"]);
    }

    // ── XSS Sanitization Tests ──────────────────────────────────────────

    #[test]
    fn test_xss_script_tag_stripped() {
        let parser = MarkdownParser::new();
        let markdown = r#"<script>alert("xss")</script>"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(
            !result.content.contains("<script"),
            "Script tags must be stripped by ammonia sanitization, got: {}",
            result.content
        );
        assert!(
            !result.content.contains("alert"),
            "Script content must be stripped, got: {}",
            result.content
        );
    }

    #[test]
    fn test_xss_event_handler_stripped() {
        let parser = MarkdownParser::new();
        // pulldown-cmark treats raw HTML as Event::Html, which ammonia then sanitizes
        let markdown = r#"<img src=x onerror="alert('xss')">"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(
            !result.content.contains("onerror"),
            "Event handlers must be stripped by ammonia, got: {}",
            result.content
        );
    }

    #[test]
    fn test_xss_javascript_uri_stripped() {
        let parser = MarkdownParser::new();
        let markdown = r#"[click me](javascript:alert('xss'))"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(
            !result.content.contains("javascript:"),
            "javascript: URIs must be stripped by ammonia, got: {}",
            result.content
        );
    }

    #[test]
    fn test_xss_iframe_stripped() {
        let parser = MarkdownParser::new();
        let markdown = r#"<iframe src="https://evil.com" onload="alert('xss')"></iframe>"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        // iframe is now allowed for embeds, but event handlers must be stripped
        assert!(
            !result.content.contains("onload"),
            "Event handlers must be stripped by ammonia, got: {}",
            result.content
        );
        // iframe src is preserved (for embeds)
        assert!(
            result.content.contains("<iframe"),
            "iframe should be preserved for embeds, got: {}",
            result.content
        );
    }

    #[test]
    fn test_xss_svg_onload_stripped() {
        let parser = MarkdownParser::new();
        let markdown = r#"<svg onload="alert('xss')"><circle r="40"/></svg>"#;
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(
            !result.content.contains("onload"),
            "SVG onload handlers must be stripped by ammonia, got: {}",
            result.content
        );
    }

    #[test]
    fn test_safe_content_preserved_after_sanitization() {
        let parser = MarkdownParser::new();
        let markdown = "# Hello\n\nParagraph with **bold** and *italic*.\n\n```rust\nfn main() {}\n```\n\n[link](https://example.com)";
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(result.content.contains("<h1>"));
        assert!(result.content.contains("<strong>bold</strong>"));
        assert!(result.content.contains("<em>italic</em>"));
        assert!(result.content.contains("<code"));
        assert!(result.content.contains("href=\"https://example.com\""));
    }

    #[test]
    fn test_image_rendering() {
        let parser = MarkdownParser::new();
        let markdown = "![alt text](https://example.com/image.png)";
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();

        assert!(
            result.content.contains("<img"),
            "Expected HTML to contain <img tag, got: {}",
            result.content
        );
        assert!(
            result.content.contains("alt=\"alt text\""),
            "Expected img to preserve alt attribute"
        );
        assert!(
            result
                .content
                .contains("src=\"https://example.com/image.png\""),
            "Expected img to preserve src attribute"
        );
    }

    // ── MDX Component Passthrough (documented behavior) ─────────────────

    /// MDX-style components in markdown are treated as raw HTML by
    /// pulldown-cmark. The default ammonia allowlist strips unknown custom
    /// tags — consumers must allowlist component names they want to keep.
    #[test]
    fn test_mdx_component_stripped_by_default() {
        let parser = MarkdownParser::new();
        let markdown = "<MyComponent prop=\"x\">inner text</MyComponent>";
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();
        assert!(
            !result.content.contains("<MyComponent"),
            "unknown custom components must be sanitized away, got: {}",
            result.content
        );
    }

    /// Inline content of a stripped custom component is preserved as text.
    #[test]
    fn test_mdx_component_inline_content_preserved() {
        let parser = MarkdownParser::new();
        let markdown = "Before <MyBadge>beta</MyBadge> after";
        let result = parser.parse(markdown, OutputFormat::Html).unwrap();
        assert!(result.content.contains("Before"));
        assert!(result.content.contains("beta"));
        assert!(result.content.contains("after"));
    }

    // ── Block Reference Tests ───────────────────────────────────────────

    #[test]
    fn test_extract_block_references_basic() {
        let parser = MarkdownParser::new();
        let content = "See ![[design-specs]] for details.";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "design-specs");
        assert_eq!(refs[0].1.heading, None);
        assert_eq!(refs[0].1.block_id, None);
        assert!(!refs[0].1.reference_only);
    }

    #[test]
    fn test_extract_block_references_with_heading() {
        let parser = MarkdownParser::new();
        let content = "Embed ![[api-docs#authentication]] here.";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "api-docs");
        assert_eq!(refs[0].1.heading, Some("authentication".to_string()));
        assert_eq!(refs[0].1.block_id, None);
    }

    #[test]
    fn test_extract_block_references_with_block_id() {
        let parser = MarkdownParser::new();
        let content = "Reference ![[notes#^important-quote]] inline.";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "notes");
        assert_eq!(refs[0].1.heading, None);
        assert_eq!(refs[0].1.block_id, Some("important-quote".to_string()));
    }

    #[test]
    fn test_extract_block_references_reference_only() {
        let parser = MarkdownParser::new();
        let content = "See ![[!design-specs]] for the original.";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "design-specs");
        assert!(refs[0].1.reference_only);
    }

    #[test]
    fn test_extract_block_references_skips_code_blocks() {
        let parser = MarkdownParser::new();
        let content = "Before.\n```\n![[should-not-parse]]\n```\nAfter ![[real-ref]].";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "real-ref");
    }

    #[test]
    fn test_extract_block_references_skips_inline_code() {
        let parser = MarkdownParser::new();
        let content = "Use `![[not-a-ref]]` literally, but ![[actual-ref]] embeds.";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1.target, "actual-ref");
    }

    #[test]
    fn test_extract_block_references_multiple() {
        let parser = MarkdownParser::new();
        let content = "![[doc-a]] and ![[doc-b#intro]] and ![[doc-c#^key]]";
        let refs = parser.extract_block_references(content);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].1.target, "doc-a");
        assert_eq!(refs[1].1.target, "doc-b");
        assert_eq!(refs[1].1.heading, Some("intro".to_string()));
        assert_eq!(refs[2].1.target, "doc-c");
        assert_eq!(refs[2].1.block_id, Some("key".to_string()));
    }

    #[test]
    fn test_extract_block_references_empty() {
        let parser = MarkdownParser::new();
        let content = "No references here.";
        let refs = parser.extract_block_references(content);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_parse_block_reference_invalid() {
        assert!(parse_block_reference("").is_none());
        assert!(parse_block_reference("#").is_none());
    }

    // ── TOC Extraction Tests ────────────────────────────────────────────

    #[test]
    fn test_extract_toc_levels() {
        let content = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let toc = extract_toc(content);
        assert_eq!(toc.len(), 6);
        let levels: Vec<usize> = toc.iter().map(|e| e.level).collect();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(toc[0].text, "H1");
        assert_eq!(toc[5].text, "H6");
    }

    #[test]
    fn test_extract_toc_nesting_order() {
        let content = "# Intro\n\n## Setup\n\n### Prerequisites\n\n### Install\n\n## Usage\n\n### CLI\n\n# Outro";
        let toc = extract_toc(content);
        let texts: Vec<&str> = toc.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Intro",
                "Setup",
                "Prerequisites",
                "Install",
                "Usage",
                "CLI",
                "Outro"
            ]
        );
        // Level sequence reflects document nesting
        let levels: Vec<usize> = toc.iter().map(|e| e.level).collect();
        assert_eq!(levels, vec![1, 2, 3, 3, 2, 3, 1]);
        // A level-3 entry follows its level-2 parent
        assert!(toc[2].level > toc[1].level);
        assert_eq!(toc[2].slug, "prerequisites");
    }

    #[test]
    fn test_extract_toc_skips_code_blocks() {
        let content = "# Real\n\n```\n# not a heading\n```\n\n## Also Real";
        let toc = extract_toc(content);
        let texts: Vec<&str> = toc.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, vec!["Real", "Also Real"]);
    }

    #[test]
    fn test_extract_toc_slugification() {
        let toc = extract_toc("## My Cool Feature (v2)!");
        assert_eq!(toc[0].slug, "my-cool-feature--v2--");
        assert_eq!(toc[0].text, "My Cool Feature (v2)!");
    }

    #[test]
    fn test_extract_toc_requires_space_after_hashes() {
        let content = "#tag not a heading\n\n# Real Heading";
        let toc = extract_toc(content);
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].text, "Real Heading");
    }

    // ── HTML TOC Tests ──────────────────────────────────────────────────

    #[test]
    fn test_extract_toc_from_html() {
        let html = r#"<h2 id="intro">Intro</h2><p>text</p><h3 id="setup">Setup &amp; <em>Config</em></h3>"#;
        let toc = extract_toc_from_html(html);
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].level, 2);
        assert_eq!(toc[0].id, "intro");
        assert_eq!(toc[0].title, "Intro");
        assert_eq!(toc[1].id, "setup");
        assert_eq!(toc[1].title, "Setup & Config");
    }

    #[test]
    fn test_extract_inline_toc_only_h2_h3() {
        let html = r#"<h1 id="a">A</h1><h2 id="b">B</h2><h3 id="c">C</h3><h4 id="d">D</h4>"#;
        let toc = extract_inline_toc(html);
        let ids: Vec<&str> = toc.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "c"]);
    }

    // ── Convenience Function Tests ──────────────────────────────────────

    #[test]
    fn test_render_markdown_free_function() {
        let html = render_markdown("# Free Function\n\nBody **bold**.");
        assert!(html.contains("<h1>"));
        assert!(html.contains("Free Function"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn test_try_render_markdown_options() {
        // Individual flags apply when the GFM bundle is off.
        let opts = MarkdownOptions {
            enable_gfm: false,
            enable_tables: false,
            ..MarkdownOptions::default()
        };
        let result = try_render_markdown("| a | b |\n|---|---|\n| 1 | 2 |", &opts).unwrap();
        assert!(!result.content.contains("<table"));
    }

    #[test]
    fn test_ammonia_allows_class_on_code() {
        let html = r#"<pre><code class="language-json">{"key": "value"}</code></pre>"#;
        let cleaned = ammonia::Builder::default()
            .add_tags(["img", "pre", "code", "span", "div"])
            .add_generic_attributes(&["class"])
            .add_tag_attributes("img", ["src", "alt", "title", "width", "height", "loading"])
            .clean(html)
            .to_string();
        assert!(
            cleaned.contains(r#"class="language-json""#),
            "Expected class preserved, got: {}",
            cleaned
        );
    }

    #[test]
    fn test_code_block_preserves_class_attribute() {
        let md = r#"```json
{"key": "value"}
```"#;
        let parser = MarkdownParser::new();
        let result = parser.parse(md, OutputFormat::Html).unwrap();
        assert!(
            result.content.contains(r#"class="language-json""#),
            "Expected code block to have class=\"language-json\", got: {}",
            &result.content
        );
    }
}
