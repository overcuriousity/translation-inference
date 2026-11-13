use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

/// Renders Markdown to HTML. Raw HTML in the input is shown as text, never as markup, and links
/// keep only web and mail addresses, so a translation cannot inject anything into the page.
pub fn render(text: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(text, opts).map(|e| match e {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            let dest_url = if safe_link(&dest_url) { dest_url } else { CowStr::Borrowed("") };
            Event::Start(Tag::Link { link_type, dest_url, title, id })
        }
        other => other,
    });
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, events);
    out
}

fn safe_link(dest: &str) -> bool {
    let lower = dest.trim().to_ascii_lowercase();
    match lower.split_once(':') {
        Some((scheme, _)) if !scheme.contains(['/', '?', '#']) => matches!(scheme, "http" | "https" | "mailto"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn markdown_becomes_html() {
        assert_eq!(render("**a**"), "<p><strong>a</strong></p>\n");
    }

    #[test]
    fn raw_html_is_escaped() {
        let out = render("<img src=x onerror=alert(1)>\n\n<script>x</script>");
        assert!(!out.contains("<img") && !out.contains("<script"));
        assert!(out.contains("&lt;img"));
    }

    #[test]
    fn script_links_lose_their_address() {
        assert!(!render("[x](javascript:alert(1))").contains("javascript"));
        assert!(!render("[x]( JavaScript:alert(1))").to_lowercase().contains("javascript"));
        assert!(render("[x](https://example.org)").contains("href=\"https://example.org\""));
        assert!(render("[x](#part)").contains("href=\"#part\""));
    }
}
