#![no_main]

use docs_pipeline::{
    extract_toc, extract_toc_from_html, highlight_code_blocks, render_markdown, sanitize_html,
    strip_html_tags, try_render_markdown, MarkdownOptions, SyntaxTheme,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Bound input so render attempts stay fast. Markdown is untrusted input;
    // the pipeline is a classic DoS surface (pathological nesting, huge
    // tables, wikilink/admonition abuse).
    let s = String::from_utf8_lossy(&data[..data.len().min(64 * 1024)]);

    // Typed render path: Err, never panic.
    let _ = try_render_markdown(&s, &MarkdownOptions::default());

    // Infallible render and extraction helpers must simply not panic.
    let html = render_markdown(&s);
    let _ = extract_toc(&s);
    let _ = extract_toc_from_html(&html);
    let _ = strip_html_tags(&html);

    // Highlighting over rendered output and raw adversarial input.
    let _ = highlight_code_blocks(&html, SyntaxTheme::default());
    let _ = highlight_code_blocks(&s, SyntaxTheme::default());

    // Sanitization must not panic on arbitrary (possibly malformed) HTML.
    let _ = sanitize_html(&html);
    let _ = sanitize_html(&s);
});
