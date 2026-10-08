//! Asking a session's host for the GitHub token of the account its project acts as.
//!
//! The token is **not** in the asker's environment and not in a file: it is requested over the
//! session's own toolcall socket (`TDDY_SOCKET`), per call, and the host resolves the project's
//! account (`tddy_accounts::acting_identity`) behind its [`GithubCredentialHandler`]. A refusal — no
//! account assigned, an account the host does not hold, a locked vault — arrives as the host's own
//! words. Nothing here reads `GITHUB_TOKEN` or `GH_TOKEN`, and with no socket there is no token: the
//! absence of a host to ask is an error, not a cue to look elsewhere.
//!
//! [`GithubCredentialHandler`]: super::GithubCredentialHandler

use std::path::Path;

use super::dispatch_toolcall;

/// [`request_github_token`] of the host the calling process's own session names in `TDDY_SOCKET` —
/// for code running inside a managed session that is not handed the socket.
///
/// The variable names a *host to ask*, not a credential. With none set there is no token.
pub async fn request_github_token_from_session() -> Result<String, String> {
    let socket = std::env::var_os("TDDY_SOCKET").map(std::path::PathBuf::from);
    request_github_token(socket.as_deref()).await
}

/// The token of the account the session's project acts as, from the host listening on `socket`.
///
/// `Err` carries the reason there is none, verbatim from the host when it refused.
pub async fn request_github_token(socket: Option<&Path>) -> Result<String, String> {
    let socket = socket.ok_or_else(|| {
        "TDDY_SOCKET is not set, so there is no session host to ask for this project's GitHub \
         account; GitHub operations only work in a managed session"
            .to_string()
    })?;
    let response = dispatch_toolcall(socket, serde_json::json!({ "type": "github-token" })).await?;
    match response["status"].as_str() {
        Some("ok") => response["token"]
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "the session host answered with no GitHub token".to_string()),
        Some("error") => Err(response["message"]
            .as_str()
            .filter(|message| !message.is_empty())
            .unwrap_or("the session host refused to resolve a GitHub account")
            .to_string()),
        _ => Err("the session host sent an unrecognised answer to github-token".to_string()),
    }
}
