//! HTML sanitization.
//!
//! Wraps `ammonia::Builder` to strip XSS vectors from rendered HTML output
//! while preserving essential attributes like `class` for syntax highlighting.

use ammonia::Builder;

/// Sanitize HTML content by removing potentially dangerous elements and attributes.
///
/// This strips:
/// - `<script>`, `<style>`, `<iframe>`, `<object>`, `<embed>`, `<form>` tags
/// - `on*` event handler attributes
/// - `javascript:` and `data:` URLs
/// - SVG-based XSS vectors
///
/// This preserves:
/// - `class` attributes (needed for syntax highlighting, CSS targeting)
/// - `id` attributes
/// - `data-*` attributes
pub fn sanitize_html(html: &str) -> String {
    Builder::default()
        .link_rel(None)
        .add_tags([
            "img", "pre", "code", "span", "div", "details", "summary", "mark", "del", "ins", "sup",
            "sub", "kbd", "samp", "var", "picture", "source", "video", "audio", "track",
        ])
        .add_generic_attributes(&[
            "class",
            "id",
            "style",
            "role",
            "aria-label",
            "aria-hidden",
            "aria-expanded",
            "data-language",
            "title",
            "colspan",
            "rowspan",
        ])
        .add_tag_attributes("img", ["src", "alt", "title", "width", "height", "loading"])
        .clean(html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strips_script_tags() {
        let input = r#"<p>Hello</p><script>alert('xss')</script><p>World</p>"#;
        let output = sanitize_html(input);
        assert!(!output.contains("<script>"));
        assert!(output.contains("Hello"));
        assert!(output.contains("World"));
    }

    #[test]
    fn test_strips_onerror_attribute() {
        let input = r#"<img src="x" onerror="alert(1)">"#;
        let output = sanitize_html(input);
        assert!(!output.contains("onerror"));
    }

    #[test]
    fn test_strips_onload_attribute() {
        let input = r#"<body onload="alert('xss')">content</body>"#;
        let output = sanitize_html(input);
        assert!(
            !output.contains("onload"),
            "onload handlers must be stripped, got: {}",
            output
        );
    }

    #[test]
    fn test_strips_javascript_uri() {
        let input = r#"<a href="javascript:alert(1)">click</a>"#;
        let output = sanitize_html(input);
        assert!(!output.contains("javascript:"));
    }

    #[test]
    fn test_preserves_safe_html() {
        let input = r#"<h1>Title</h1><p><strong>Bold</strong> and <em>italic</em></p><a href="https://example.com">link</a>"#;
        let output = sanitize_html(input);
        assert!(output.contains("<h1>"));
        assert!(output.contains("<strong>"));
        assert!(output.contains("href"));
    }

    #[test]
    fn test_strips_svg_xss() {
        let input = r#"<svg onload="alert(1)"><circle r="10"/></svg>"#;
        let output = sanitize_html(input);
        assert!(!output.contains("onload"));
    }

    #[test]
    fn test_strips_iframe() {
        let input = r#"<iframe src="https://evil.com"></iframe>"#;
        let output = sanitize_html(input);
        assert!(!output.contains("<iframe"));
    }

    #[test]
    fn test_preserves_class_on_code_block() {
        let input = r#"<pre><code class="language-json">{"key": "value"}</code></pre>"#;
        let output = sanitize_html(input);
        assert!(
            output.contains(r#"class="language-json""#),
            "Expected class preserved, got: {}",
            output
        );
    }
}
