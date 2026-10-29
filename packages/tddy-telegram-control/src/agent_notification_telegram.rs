//! An agent-written notification as the Telegram messages that carry it.
//!
//! The layout is fixed — the title in bold, the session label in italics, the markdown body
//! rendered to Telegram's HTML subset, then the session link — and split into as many messages as
//! Telegram's 4,096-byte limit needs, each of them well-formed HTML on its own, because the Bot API
//! refuses a message with an unclosed tag outright.
//!
//! A module of its own rather than more of `telegram_notifier.rs` or
//! `telegram_session_subscriber.rs`: both are over budget, and rendering is a pure function of the
//! notification with nothing to share with the presenter path's plain-text chunking.

use tddy_daemon_kernel::notification_markdown::{escape_html, to_telegram_html};
use tddy_daemon_kernel::session_notification::SessionNotification;
use tddy_telegram::sender::TelegramSender;

/// The most a single Telegram message may carry, in bytes of UTF-8.
pub const TELEGRAM_HTML_MESSAGE_MAX_BYTES: usize = 4096;

/// The HTML messages one agent notification is sent as, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNotificationTelegram {
    messages: Vec<String>,
    silent: bool,
}

impl AgentNotificationTelegram {
    /// The messages `notification` is sent as, or `None` when it carries no agent-written content
    /// — a status notification is sent as its `text`, not rendered.
    #[must_use]
    pub fn render(notification: &SessionNotification) -> Option<Self> {
        let agent = notification.agent.as_ref()?;
        let header = format!(
            "<b>{}</b>\n<i>{}</i>",
            escape_html(&agent.title),
            escape_html(&notification.label)
        );
        let sections = [
            header,
            to_telegram_html(&agent.body_markdown),
            escape_html(&agent.link),
        ];
        let html = sections
            .iter()
            .filter(|section| !section.is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n\n");
        Some(Self {
            messages: split_html(&html, TELEGRAM_HTML_MESSAGE_MAX_BYTES),
            silent: !agent.sound,
        })
    }

    /// Send every message to each of `chat_ids`, in order, silently or not as the agent asked.
    ///
    /// A chat that refuses a message is not sent the rest of this notification — the tail of a
    /// split body without its head is worse than none — and the others still get theirs. Fails with
    /// how many chats were missed, without the underlying error text, which is logged here.
    pub async fn send_to(
        &self,
        sender: &dyn TelegramSender,
        chat_ids: &[i64],
    ) -> anyhow::Result<()> {
        let mut failed = 0_usize;
        for &chat_id in chat_ids {
            for html in &self.messages {
                if let Err(e) = sender.send_html_message(chat_id, html, self.silent).await {
                    failed += 1;
                    log::warn!(
                        target: "tddy_daemon::telegram",
                        "send_html_message failed chat_id={chat_id}: {e:#}"
                    );
                    break;
                }
            }
        }
        if failed > 0 {
            anyhow::bail!(
                "telegram send failed for {failed} of {} chat(s)",
                chat_ids.len()
            );
        }
        Ok(())
    }

    /// The HTML messages, in send order; each at most [`TELEGRAM_HTML_MESSAGE_MAX_BYTES`].
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Whether they are sent with `disable_notification` — the agent did not ask for a sound.
    #[must_use]
    pub fn silent(&self) -> bool {
        self.silent
    }
}

/// One indivisible piece of an HTML message: a tag, or a unit of text — a character or an entity.
enum Atom<'a> {
    Open { name: &'a str, tag: &'a str },
    Close(&'a str),
    Text(&'a str),
}

impl Atom<'_> {
    fn as_str(&self) -> &str {
        match self {
            Atom::Open { tag, .. } | Atom::Close(tag) => tag,
            Atom::Text(text) => text,
        }
    }

    fn is_whitespace(&self) -> bool {
        matches!(self, Atom::Text(text) if text.trim().is_empty())
    }
}

/// `html` — which only ever holds this crate's own tags and escaped text — as its atoms.
fn atoms_of(html: &str) -> Vec<Atom<'_>> {
    let mut atoms = Vec::new();
    let mut rest = html;
    while let Some(first) = rest.chars().next() {
        let unit_len = match first {
            '<' => rest.find('>').map_or(1, |end| end + 1),
            '&' => rest
                .find(';')
                .filter(|end| *end <= ENTITY_MAX_BYTES)
                .map_or(1, |end| end + 1),
            other => other.len_utf8(),
        };
        let (unit, tail) = rest.split_at(unit_len);
        atoms.push(match first {
            '<' if unit_len > 1 && unit[1..].starts_with('/') => Atom::Close(unit),
            '<' if unit_len > 1 => Atom::Open {
                name: unit[1..unit_len - 1]
                    .split_whitespace()
                    .next()
                    .unwrap_or_default(),
                tag: unit,
            },
            _ => Atom::Text(unit),
        });
        rest = tail;
    }
    atoms
}

/// The longest entity this module's escaping emits (`&quot;`), less its `&`.
const ENTITY_MAX_BYTES: usize = 5;

/// Where a message can end at whitespace: its length there, the tags open there, and the atom to
/// resume at.
type LastBreak<'a> = (usize, Vec<(&'a str, &'a str)>, usize);

/// `html` as messages of at most `limit` bytes, each well-formed on its own: a message that ends
/// inside an open tag closes it, and the next reopens it. Breaks fall at whitespace when the
/// message has any, and never inside a tag, an entity or a character.
pub(crate) fn split_html(html: &str, limit: usize) -> Vec<String> {
    let atoms = atoms_of(html);
    let mut messages = Vec::new();
    let mut reopen: Vec<(&str, &str)> = Vec::new();
    let mut next = 0;
    while next < atoms.len() {
        let mut message: String = reopen.iter().map(|(_, tag)| *tag).collect();
        let mut open = reopen.clone();
        let carried_len = message.len();
        // Where the message can end instead if the next atom does not fit: after the last
        // whitespace, with what was open there.
        let mut last_break: Option<LastBreak<'_>> = None;
        while let Some(atom) = atoms.get(next) {
            let mut after = open.clone();
            match atom {
                Atom::Open { name, tag } => after.push((name, tag)),
                Atom::Close(_) => {
                    after.pop();
                }
                Atom::Text(_) => {}
            }
            let closing: usize = after.iter().map(|(name, _)| name.len() + 3).sum();
            if message.len() + atom.as_str().len() + closing > limit && message.len() > carried_len
            {
                if let Some((len, open_at_break, resume)) = last_break.take() {
                    message.truncate(len);
                    open = open_at_break;
                    next = resume;
                }
                break;
            }
            message.push_str(atom.as_str());
            open = after;
            next += 1;
            if atom.is_whitespace() {
                last_break = Some((message.len(), open.clone(), next));
            }
        }
        for (name, _) in open.iter().rev() {
            message.push_str(&format!("</{name}>"));
        }
        messages.push(message);
        reopen = open;
    }
    messages
}
