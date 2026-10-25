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

/// `md` as Telegram HTML: bold, italic, inline code, code blocks and links render; every other
/// construct is flattened to text, and every `<`, `>` and `&` that is not one of those tags is
/// escaped.
#[must_use]
pub fn to_telegram_html(md: &str) -> String {
    // TODO(agent-notify): implement
    let _ = md;
    String::new()
}

/// `md` with its markup removed, at most `max_chars` characters long. A cut body ends in `…`, and
/// the cut never splits a character.
#[must_use]
pub fn to_plain_text(md: &str, max_chars: usize) -> String {
    // TODO(agent-notify): implement
    let _ = (md, max_chars);
    String::new()
}
