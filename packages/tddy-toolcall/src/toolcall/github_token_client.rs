//! Asking a session's host for the GitHub token of the account its project acts as.
//!
//! The token is **not** in the asker's environment and not in a file: it is requested over the
//! session's own toolcall socket (`TDDY_SOCKET`), per call, and the host resolves the project's
//! account (`tddy_accounts::acting_identity`) behind its [`GithubCredentialHandler`]. A refusal — no
//! account assigned, an account the host does not hold, a locked vault — arrives as the host's own
//! words. Nothing here reads `GITHUB_TOKEN` or `GH_TOKEN`, and with no socket there is no token: the
//! absence of a host to ask is an error, not a cue to look elsewhere.
//!
//! A session whose project uses **Local gh** is told so by [`GITHUB_SOURCE_ENV`] instead
//! (`local-gh:<github host>`). That value names a host to ask `gh` about, not a credential: the token
//! is read from the host's own GitHub CLI, in the asking process, as the user it already runs as, and
//! the session socket is never asked. See docs/ft/daemon/github-identity.md.
//!
//! [`GithubCredentialHandler`]: super::GithubCredentialHandler

use std::path::{Path, PathBuf};

/// The variable that tells a session's processes where their GitHub token comes from. Absent means
/// the session host (`TDDY_SOCKET`); [`local_gh_source_value`] means the host's own `gh`.
pub const GITHUB_SOURCE_ENV: &str = "TDDY_GITHUB_SOURCE";

/// The [`GITHUB_SOURCE_ENV`] value for a Local gh session whose project lives on `hostname`.
pub fn local_gh_source_value(hostname: &str) -> String {
    todo!("local_gh_source_value")
}

/// The GitHub host a [`GITHUB_SOURCE_ENV`] value names, or why the value is not one this code
/// writes. A malformed value is an error, never a reason to ask somewhere else.
pub fn parse_github_source(value: &str) -> Result<String, String> {
    todo!("parse_github_source")
}

/// Where the calling process gets its GitHub token from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTokenSource {
    /// Ask the session host listening on this socket (`TDDY_SOCKET`); `None` when there is none.
    HostSocket(Option<PathBuf>),
    /// Ask `gh auth token --hostname <hostname>`, running `gh_program` (`None`: no `gh` on `PATH`).
    LocalGh {
        hostname: String,
        gh_program: Option<PathBuf>,
    },
}

impl SessionTokenSource {
    /// The source the calling process's own session names: [`GITHUB_SOURCE_ENV`] when set (with
    /// `gh` looked up on `PATH`), the `TDDY_SOCKET` host otherwise. [`Self::resolve`] over the
    /// process environment.
    pub fn from_environment() -> Result<Self, String> {
        todo!("SessionTokenSource::from_environment")
    }

    /// The source a session's environment names: `github_source` (the [`GITHUB_SOURCE_ENV`] value)
    /// when present, with `gh_program` as the `gh` it would run; the host at `tddy_socket`
    /// otherwise. A `github_source` that is not one this code writes is an error.
    pub fn resolve(
        github_source: Option<&str>,
        tddy_socket: Option<PathBuf>,
        gh_program: Option<PathBuf>,
    ) -> Result<Self, String> {
        todo!("SessionTokenSource::resolve")
    }
}

/// The token `source` answers with. For [`SessionTokenSource::LocalGh`] that is `gh`'s, with
/// `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` and `GITHUB_ENTERPRISE_TOKEN` withheld from it;
/// the session host is not asked. `Err` carries the reason there is none, in the answerer's words.
pub async fn request_github_token_from(source: &SessionTokenSource) -> Result<String, String> {
    todo!("request_github_token_from")
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_source_value_names_local_gh_and_its_host() {
        // Given the value a Local gh session on an Enterprise host is started with
        let value = local_gh_source_value("github.example.com");

        // When it is read back
        let hostname = parse_github_source(&value);

        // Then it is that exact wire value, naming that host
        assert_eq!(
            (value.as_str(), hostname),
            ("local-gh:github.example.com", Ok("github.example.com".to_string()))
        );
    }

    #[test]
    fn a_github_source_naming_no_host_is_refused() {
        // Given a Local gh value with the host missing
        // When it is read
        let refusal = parse_github_source("local-gh:");

        // Then it is refused, naming the value
        let message = refusal.expect_err("a source names a host");
        assert!(message.contains("local-gh:"), "got: {message}");
    }

    #[test]
    fn a_session_with_no_github_source_asks_its_session_host() {
        // Given a session started with a host socket and no GitHub source
        let socket = PathBuf::from("/run/tddy/session-7.sock");

        // When its token source is resolved
        let source = SessionTokenSource::resolve(None, Some(socket.clone()), None);

        // Then it is that host
        assert_eq!(source, Ok(SessionTokenSource::HostSocket(Some(socket))));
    }

    #[test]
    fn a_session_told_local_gh_asks_gh_about_its_host_and_not_the_socket() {
        // Given a session told Local gh on an Enterprise host, which also has a host socket
        let gh = PathBuf::from("/opt/homebrew/bin/gh");

        // When its token source is resolved
        let source = SessionTokenSource::resolve(
            Some("local-gh:github.example.com"),
            Some(PathBuf::from("/run/tddy/session-7.sock")),
            Some(gh.clone()),
        );

        // Then it is gh, for that host
        assert_eq!(
            source,
            Ok(SessionTokenSource::LocalGh {
                hostname: "github.example.com".to_string(),
                gh_program: Some(gh),
            })
        );
    }
}
