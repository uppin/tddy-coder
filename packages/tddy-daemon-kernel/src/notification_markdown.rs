//! An agent notification's markdown body, rendered for the surfaces that show it.
//!
//! Two renderings of one input, built on `pulldown-cmark`:
//!
//! - [`to_telegram_html`] — Telegram's HTML parse mode accepts only a small tag set, and rejects a
//!   whole message over one unknown tag or one unescaped `<`. So only `b`, `i`, `code`, `pre` and
//!   `a` are emitted, and everything else — raw HTML in the body included — is escaped text.
//! - [`to_plain_text`] — a desktop notification renders no markup at all, and truncates; the body is
//!   flattened to text and cut at a character boundary.
//!
//! Kernel-owned rather than Telegram's, because more than one surface renders the same body
//! (`#home-inbox` 4/8 shows it on the desktop).

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// `md` as Telegram HTML: bold, italic, inline code, code blocks and links render; every other
/// construct is flattened to text, and every `<`, `>` and `&` that is not one of those tags is
/// escaped.
#[must_use]
pub fn to_telegram_html(md: &str) -> String {
    let mut out = String::new();
    let mut list_depth = 0usize;
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::Paragraph | Tag::CodeBlock(_) | Tag::HtmlBlock) => {
                if list_depth == 0 {
                    block_separator(&mut out);
                }
                if matches!(event, Event::Start(Tag::CodeBlock(_))) {
                    out.push_str("<pre>");
                }
            }
            Event::Start(Tag::Heading { .. }) => {
                block_separator(&mut out);
                out.push_str("<b>");
            }
            Event::End(TagEnd::CodeBlock) => {
                out.truncate(out.trim_end_matches('\n').len());
                out.push_str("</pre>");
            }
            Event::Start(Tag::List(_)) => {
                if list_depth == 0 {
                    block_separator(&mut out);
                }
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => list_depth -= 1,
            Event::Start(Tag::Item) => start_list_item(&mut out, list_depth),
            inline => push_inline(&mut out, inline),
        }
    }
    out.truncate(out.trim_end().len());
    out
}

/// A list item's bullet, on a line of its own, indented two spaces per level below the first.
fn start_list_item(out: &mut String, list_depth: usize) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
    out.push_str("• ");
}

/// One inline event of [`to_telegram_html`]: the tags Telegram accepts, escaped text, and nothing
/// for every construct it has no tag for.
fn push_inline(out: &mut String, event: Event<'_>) {
    match event {
        Event::End(TagEnd::Heading(_) | TagEnd::Strong) => out.push_str("</b>"),
        Event::Start(Tag::Strong) => out.push_str("<b>"),
        Event::Start(Tag::Emphasis) => out.push_str("<i>"),
        Event::End(TagEnd::Emphasis) => out.push_str("</i>"),
        Event::Start(Tag::Link { dest_url, .. }) => {
            out.push_str("<a href=\"");
            out.push_str(&escape_html(&dest_url));
            out.push_str("\">");
        }
        Event::End(TagEnd::Link) => out.push_str("</a>"),
        Event::Code(code) => {
            out.push_str("<code>");
            out.push_str(&escape_html(&code));
            out.push_str("</code>");
        }
        Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
            out.push_str(&escape_html(&text));
        }
        Event::SoftBreak | Event::HardBreak => out.push('\n'),
        _ => {}
    }
}

/// `md` with its markup removed, at most `max_chars` characters long. A cut body ends in `…`, and
/// the cut never splits a character.
#[must_use]
pub fn to_plain_text(md: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::Item)
            | Event::Start(Tag::HtmlBlock) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                out.push_str(&text);
            }
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            _ => {}
        }
    }
    let out = out.trim_end();
    if out.chars().count() <= max_chars {
        return out.to_string();
    }
    let mut cut: String = out.chars().take(max_chars.saturating_sub(1)).collect();
    if max_chars > 0 {
        cut.push('…');
    }
    cut
}

/// A blank line between two blocks, and nothing before the first.
fn block_separator(out: &mut String) {
    if !out.is_empty() {
        out.push_str("\n\n");
    }
}

/// `text` with the three characters Telegram's HTML parse mode reads as markup escaped, and the
/// quote that would end an attribute. For the words of a notification that are not markdown — its
/// title, its session label — where [`to_telegram_html`] would read markup into them.
#[must_use]
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            other => escaped.push(other),
        }
    }
    escaped
}
