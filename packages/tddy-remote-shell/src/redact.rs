//! What a call may say about itself in an error or a log: the caller's command, cut short, with
//! no credential in it (`#coder-start` 8/10).

/// The longest command head an error or a report keeps, in characters.
pub const COMMAND_HEAD_CHARS: usize = 200;

/// The longest stderr tail a report keeps, in characters.
pub const STDERR_TAIL_CHARS: usize = 400;

const REDACTED: &str = "***";

/// `text` with the user-info of every `scheme://user:password@host` URL replaced by `***`, so a
/// credentialed URL (a token, a user and password) never reaches a log or an error.
pub fn scrub_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let authority_start = at + 3;
        out.push_str(&rest[..authority_start]);
        rest = &rest[authority_start..];
        let authority_end = rest
            .find(|c: char| c == '/' || c.is_whitespace() || matches!(c, '\'' | '"' | '?' | '#'))
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        match authority.rfind('@') {
            Some(at_sign) => {
                out.push_str(REDACTED);
                out.push_str(&authority[at_sign..]);
            }
            None => out.push_str(authority),
        }
        rest = &rest[authority_end..];
    }
    out.push_str(rest);
    out
}

/// The first `max_chars` characters of `text`, marked when cut.
pub fn head(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text.to_string(),
    }
}

/// The last `max_chars` characters of `text`, marked when cut.
pub fn tail(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let (cut, _) = text
        .char_indices()
        .nth(total - max_chars)
        .unwrap_or((0, ' '));
    format!("...{}", &text[cut..])
}

/// The caller's command as an error or a report may name it: scrubbed, then cut short.
pub fn command_head(command: &str) -> String {
    head(&scrub_credentials(command), COMMAND_HEAD_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_url_loses_its_user_info_and_keeps_its_host_and_path() {
        assert_eq!(
            scrub_credentials("git clone https://bot:s3cret@example.com/a/b.git /srv"),
            "git clone https://***@example.com/a/b.git /srv"
        );
    }

    #[test]
    fn a_url_without_user_info_is_unchanged() {
        let text = "fatal: unable to access 'https://example.com/a/b.git/'";
        assert_eq!(scrub_credentials(text), text);
    }

    #[test]
    fn a_long_command_is_cut_at_a_character_boundary() {
        let long = "é".repeat(COMMAND_HEAD_CHARS + 5);
        assert_eq!(
            command_head(&long),
            format!("{}...", "é".repeat(COMMAND_HEAD_CHARS))
        );
    }
}
