//! Text Codex's sign-in and session files are read from and written to.

use std::path::Path;

use super::CODEX_THREAD_ID_FILENAME;

/// Extract an `https://…` URL from a line of `codex login` output (or any single-line text).
#[must_use]
pub fn scrape_codex_oauth_authorize_url_from_text(s: &str) -> Option<String> {
    let s = s.trim();
    let start = s.find("https://")?;
    let rest = &s[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let mut url = rest[..end].to_string();
    while url.ends_with(')') || url.ends_with('"') || url.ends_with('\'') || url.ends_with(',') {
        url.pop();
    }
    (url.len() > "https://".len()).then_some(url)
}

/// When Codex/OpenAI returns 401-style text, append a short fix hint for the TUI.
pub(super) fn codex_openai_auth_remediation(detail: &str) -> Option<&'static str> {
    let d = detail.to_lowercase();
    if !d.contains("401") {
        return None;
    }
    if d.contains("unauthorized")
        || d.contains("missing bearer")
        || d.contains("authentication")
        || d.contains("invalid api key")
    {
        Some(
            "Fix: not signed in to OpenAI for Codex. `codex exec --json` does not print a login URL or start browser OAuth — only API errors.\n\
             Run `codex login` first (URL on stdout + BROWSER), or `tddy-coder --session-dir <artifact_dir> --codex-oauth-login` to capture the link for the web UI.\n\
             Or use an API key: `printenv OPENAI_API_KEY | codex login --with-api-key`.",
        )
    } else {
        None
    }
}

pub fn write_codex_thread_id_file(session_dir: &Path, thread_id: &str) {
    let path = session_dir.join(CODEX_THREAD_ID_FILENAME);
    match crate::atomic_file::write_atomic(&path, thread_id.trim()) {
        Ok(()) => log::debug!("[tddy-codex] persisted thread id to {}", path.display()),
        Err(e) => log::warn!("[tddy-codex] could not write {}: {}", path.display(), e),
    }
}
