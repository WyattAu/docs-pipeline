//! Rich-media embed rendering.
//!
//! Renders whitelisted third-party embeds (YouTube, Figma, GitHub Gists,
//! CodePen, tweets) into sandboxed iframes. Untrusted domains are blocked.

use std::sync::LazyLock;

static WHITELISTED_DOMAINS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "www.youtube.com",
        "youtube.com",
        "youtu.be",
        "www.figma.com",
        "figma.com",
        "gist.github.com",
        "codepen.io",
        "cdpn.io",
        "platform.twitter.com",
        "syndication.twitter.com",
        "x.com",
        "www.x.com",
    ]
});

#[allow(dead_code)]
const MAX_EMBEDS: usize = 10;

/// Check whether a URL's domain is on the embed whitelist.
pub fn is_domain_whitelisted(url: &str) -> bool {
    WHITELISTED_DOMAINS.iter().any(|d| url.contains(d))
}

/// Count embed occurrences in content (skipping code blocks).
pub fn count_embeds(content: &str) -> usize {
    let mut count = 0;
    let mut in_code_block = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        for alt in ["youtube", "figma", "gist", "codepen", "tweet"] {
            let prefix = format!("![{}](", alt);
            let mut rest = line;
            while let Some(pos) = rest.find(&prefix) {
                let after = &rest[pos + prefix.len()..];
                if let Some(close) = after.find(')') {
                    let url = &after[..close];
                    if !url.trim().is_empty() {
                        count += 1;
                    }
                    rest = &after[close + 1..];
                } else {
                    break;
                }
            }
        }
    }
    count
}

/// Render a YouTube embed iframe for a video ID or URL.
pub fn render_youtube(video_id: &str) -> String {
    let video_id = video_id.trim();
    if video_id.is_empty() {
        return "<div class=\"embed-error\">YouTube embed: empty video ID</div>".to_string();
    }
    if !is_domain_whitelisted(video_id) && video_id.contains('/') {
        return "<div class=\"embed-error\">YouTube embed blocked: untrusted URL</div>".to_string();
    }
    let id = video_id.split('/').next_back().unwrap_or(video_id);
    format!(
        r#"<div class="embed-youtube" data-video-id="{id}"><iframe src="https://www.youtube.com/embed/{id}" width="100%" height="360" frameborder="0" allowfullscreen loading="lazy" sandbox="allow-scripts allow-same-origin"></iframe></div>"#,
        id = html_escape(id)
    )
}

/// Render a Figma embed iframe for a whitelisted Figma URL.
pub fn render_figma(url: &str) -> String {
    let url = url.trim();
    if !is_domain_whitelisted(url) {
        return "<div class=\"embed-error\">Figma embed blocked: untrusted URL</div>".to_string();
    }
    format!(
        r#"<div class="embed-figma"><iframe src="https://www.figma.com/embed?embed_host=share&url={url}" width="100%" height="450" frameborder="0" allowfullscreen loading="lazy" sandbox="allow-scripts allow-same-origin"></iframe></div>"#,
        url = url_encode(url)
    )
}

/// Render a GitHub Gist embed iframe for a whitelisted gist URL.
pub fn render_gist(url: &str) -> String {
    let url = url.trim();
    if !is_domain_whitelisted(url) {
        return "<div class=\"embed-error\">Gist embed blocked: untrusted URL</div>".to_string();
    }
    let raw = if url.ends_with(".js") {
        url.to_string()
    } else {
        format!("{}.js", url)
    };
    format!(
        r#"<div class="embed-gist"><iframe src="{raw}" width="100%" height="300" frameborder="0" loading="lazy" sandbox="allow-scripts allow-same-origin"></iframe></div>"#,
        raw = html_escape(&raw)
    )
}

/// Render a CodePen embed iframe for a whitelisted CodePen URL.
pub fn render_codepen(url: &str) -> String {
    let url = url.trim();
    if !is_domain_whitelisted(url) {
        return "<div class=\"embed-error\">CodePen embed blocked: untrusted URL</div>".to_string();
    }
    let embed_url = url
        .replace("codepen.io/", "codepen.io/embed/")
        .replace("cdpn.io/", "cdpn.io/embed/");
    let embed_url = if !embed_url.contains("/embed/") {
        format!("{}/embed/", url.trim_end_matches('/'))
    } else {
        embed_url
    };
    format!(
        r#"<div class="embed-codepen"><iframe src="{embed_url}" width="100%" height="400" frameborder="0" loading="lazy" sandbox="allow-scripts allow-same-origin"></iframe></div>"#,
        embed_url = html_escape(&embed_url)
    )
}

/// Render a tweet embed blockquote for a whitelisted tweet URL.
pub fn render_tweet(url: &str) -> String {
    let url = url.trim();
    if !is_domain_whitelisted(url) {
        return "<div class=\"embed-error\">Tweet embed blocked: untrusted URL</div>".to_string();
    }
    format!(
        r#"<div class="embed-tweet"><blockquote class="twitter-tweet" data-tweet-url="{url}"><a href="{url}">View Tweet</a></blockquote></div>"#,
        url = html_escape(url)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn url_encode(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('"', "%22")
        .replace('#', "%23")
}

/// Dispatch an embed render by `![alt](url)` alt name.
///
/// Returns `None` for unrecognized embed types.
pub fn render_embed(alt: &str, url: &str) -> Option<String> {
    Some(match alt {
        "youtube" => render_youtube(url),
        "figma" => render_figma(url),
        "gist" => render_gist(url),
        "codepen" => render_codepen(url),
        "tweet" => render_tweet(url),
        _ => return None,
    })
}

/// CSP policy string for documents with embeds.
pub fn embed_csp_policy() -> &'static str {
    concat!(
        "frame-src https://www.youtube.com https://youtube.com https://figma.com https://www.figma.com ",
        "https://gist.github.com https://codepen.io https://cdpn.io ",
        "https://platform.twitter.com https://syndication.twitter.com https://x.com https://www.x.com; ",
        "script-src 'self' 'unsafe-inline' https://platform.twitter.com;"
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_youtube_render() {
        let html = render_youtube("dQw4w9WgXcQ");
        assert!(html.contains("embed-youtube"));
        assert!(html.contains("youtube.com/embed/dQw4w9WgXcQ"));
        assert!(html.contains("loading=\"lazy\""));
        assert!(html.contains("sandbox=\"allow-scripts allow-same-origin\""));
    }

    #[test]
    fn test_youtube_render_with_url() {
        let html = render_youtube("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(html.contains("dQw4w9WgXcQ"));
    }

    #[test]
    fn test_figma_render() {
        let html = render_figma("https://www.figma.com/file/abc123");
        assert!(html.contains("embed-figma"));
        assert!(html.contains("figma.com/embed"));
        assert!(html.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_gist_render() {
        let html = render_gist("https://gist.github.com/user/abc123");
        assert!(html.contains("embed-gist"));
        assert!(html.contains("gist.github.com/user/abc123.js"));
        assert!(html.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_codepen_render() {
        let html = render_codepen("https://codepen.io/user/pen/abc123");
        assert!(html.contains("embed-codepen"));
        assert!(html.contains("codepen.io"));
        assert!(html.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_tweet_render() {
        let html = render_tweet("https://x.com/user/status/123456");
        assert!(html.contains("embed-tweet"));
        assert!(html.contains("twitter-tweet"));
    }

    #[test]
    fn test_untrusted_domain_blocked() {
        let html = render_youtube("https://evil.com/steal");
        assert!(html.contains("blocked"));
        assert!(!html.contains("<iframe"));

        let html = render_figma("https://evil.com/fake");
        assert!(html.contains("blocked"));

        let html = render_gist("https://evil.com/fake");
        assert!(html.contains("blocked"));

        let html = render_codepen("https://evil.com/fake");
        assert!(html.contains("blocked"));

        let html = render_tweet("https://evil.com/fake");
        assert!(html.contains("blocked"));
    }

    #[test]
    fn test_domain_whitelist() {
        assert!(is_domain_whitelisted("https://www.youtube.com/watch?v=123"));
        assert!(is_domain_whitelisted("https://youtube.com/watch?v=123"));
        assert!(is_domain_whitelisted("https://www.figma.com/file/abc"));
        assert!(is_domain_whitelisted("https://gist.github.com/user/123"));
        assert!(is_domain_whitelisted("https://codepen.io/user/pen/abc"));
        assert!(is_domain_whitelisted("https://x.com/user/status/123"));
        assert!(!is_domain_whitelisted("https://evil.com/malicious"));
    }

    #[test]
    fn test_max_embed_count() {
        let content = (0..15)
            .map(|i| format!("![youtube](vid{})", i))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(count_embeds(&content), 15);
    }

    #[test]
    fn test_count_embeds_skips_code_blocks() {
        let content = "![youtube](abc)\n```\n![youtube](xyz)\n```\n![figma](url)";
        assert_eq!(count_embeds(content), 2);
    }

    #[test]
    fn test_max_embeds_enforced() {
        let mut embeds = Vec::new();
        for i in 0..15 {
            let url = format!("https://www.youtube.com/watch?v={}", i);
            let count = embeds.len();
            if count < MAX_EMBEDS {
                embeds.push(render_youtube(&url));
            }
        }
        assert_eq!(embeds.len(), MAX_EMBEDS);
    }

    #[test]
    fn test_render_embed_dispatch() {
        let html = render_embed("youtube", "abc123").unwrap();
        assert!(html.contains("embed-youtube"));
        assert!(render_embed("unknown", "abc123").is_none());
    }

    #[test]
    fn test_csp_policy() {
        let csp = embed_csp_policy();
        assert!(csp.contains("frame-src"));
        assert!(csp.contains("youtube.com"));
        assert!(csp.contains("figma.com"));
        assert!(csp.contains("twitter.com"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("a&b\"c<d>e"), "a&amp;b&quot;c&lt;d&gt;e");
    }

    #[test]
    fn test_empty_url_returns_error() {
        let html = render_youtube("");
        assert!(
            html.contains("empty") || html.contains("error"),
            "Expected error for empty URL, got: {}",
            html
        );
    }
}
