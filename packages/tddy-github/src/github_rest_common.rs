//! Shared GitHub REST API constants and curl transport for **tddy-workflow-recipes** and consumers (e.g. **tddy-tools**).
//!
//! No function in here resolves a credential. A token reaches a call only by being passed to it, and
//! a blank one is refused before curl runs: which GitHub account acts is decided by the project's
//! assignment (`tddy-accounts`), never by whatever the process environment happens to hold.
//!
//! Keep `Accept` and `X-GitHub-Api-Version` in sync across merge-pr curl and tddy-tools GitHub PR helpers.

/// GitHub REST API version header value (REST API v2022-11-28).
pub const GITHUB_API_VERSION: &str = "2022-11-28";

/// Required `Accept` header for GitHub REST JSON responses.
pub const GITHUB_ACCEPT: &str = "application/vnd.github+json";

/// `User-Agent` for merge-pr workflow curl calls (historical identifier).
pub const USER_AGENT_MERGE_PR: &str = "tddy-coder-workflow-recipes";

/// `User-Agent` for **tddy-tools** GitHub PR MCP / REST calls.
pub const USER_AGENT_TDDY_TOOLS: &str = "tddy-tools";

/// Refuse a blank credential before any process is spawned — an empty `Authorization: Bearer`
/// header is not "unauthenticated", it is a request that fails at GitHub with a less useful reason.
fn require_token(op: &str, token: &str) -> Result<(), tddy_core::WorkflowError> {
    if token.trim().is_empty() {
        return Err(curl_err(format!("{op}: no GitHub token supplied")));
    }
    Ok(())
}

/// Root of the GitHub REST API every call in this module is built from.
const GITHUB_API_BASE: &str = "https://api.github.com";

fn github_api_url(repo: &str, path: &str) -> String {
    format!("{GITHUB_API_BASE}/repos/{repo}/{path}")
}

fn temp_github_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}.json", uuid::Uuid::new_v4()))
}

fn curl_err(msg: impl Into<String>) -> tddy_core::WorkflowError {
    tddy_core::WorkflowError::WriteFailed(msg.into())
}

fn run_curl_json_body(
    url: &str,
    method: &str,
    body: &str,
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    require_token(&format!("GitHub {method}"), token)?;
    let body_path = temp_github_path("tddy-gh-req-body");
    let out_path = temp_github_path("tddy-gh-resp");
    std::fs::write(&body_path, body.as_bytes()).map_err(|e| curl_err(e.to_string()))?;

    let out = std::process::Command::new("curl")
        .arg("-sS")
        .arg("-L")
        .arg("-o")
        .arg(&out_path)
        .arg("-w")
        .arg("%{http_code}")
        .arg("-X")
        .arg(method)
        .arg("-H")
        .arg(format!("Authorization: Bearer {token}"))
        .arg("-H")
        .arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H")
        .arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"))
        .arg("--data-binary")
        .arg(format!("@{}", body_path.display()))
        .arg(url)
        .output()
        .map_err(|e| curl_err(format!("curl ({method}): {e}")))?;

    std::fs::remove_file(&body_path).ok();
    if !out.status.success() {
        std::fs::remove_file(&out_path).ok();
        return Err(curl_err(format!(
            "curl ({method}) process failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let code_str = String::from_utf8_lossy(&out.stdout);
    let http_code: u16 = code_str
        .trim()
        .parse()
        .map_err(|e| curl_err(format!("curl: invalid HTTP status {code_str:?}: {e}")))?;
    let body_raw = std::fs::read_to_string(&out_path).map_err(|e| curl_err(e.to_string()))?;
    std::fs::remove_file(&out_path).ok();
    if !(200..300).contains(&http_code) {
        return Err(curl_err(format!(
            "GitHub API {method} {url} returned HTTP {http_code}: {body_raw}"
        )));
    }
    Ok(body_raw)
}

/// HTTP PATCH with a JSON body string, returns the response body, authenticated as `token`.
pub fn curl_github_patch_json_with_token(
    repo: &str,
    path: &str,
    body: &str,
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "PATCH", body, token)
}

/// HTTP POST with a JSON body string (see [`curl_github_patch_json_with_token`]).
pub fn curl_github_post_json_with_token(
    repo: &str,
    path: &str,
    body: &str,
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "POST", body, token)
}

/// HTTP GET with query parameters (see [`curl_github_patch_json_with_token`]).
pub fn curl_github_get_json_with_token(
    repo: &str,
    path: &str,
    query: &[(&str, &str)],
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    run_curl_get(&github_api_url(repo, path), query, token)
}

/// HTTP GET against an API path that is **not** under `/repos/{owner}/{repo}/` — `/search/issues`
/// being the one this exists for.
///
/// [`curl_github_get_json_with_token`] can only address a repository sub-resource, and search is
/// repository-scoped by a `repo:` **query qualifier** rather than by its path. Same curl transport,
/// same headers, same token handling — only the URL shape differs. `path` is relative to the API
/// root, e.g. `"search/issues"`.
pub fn curl_github_get_json_absolute_path(
    path: &str,
    query: &[(&str, &str)],
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let url = format!("{GITHUB_API_BASE}/{}", path.trim_start_matches('/'));
    run_curl_get(&url, query, token)
}

fn run_curl_get(
    url: &str,
    query: &[(&str, &str)],
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    require_token("GitHub GET", token)?;
    let out_path = temp_github_path("tddy-gh-get");

    let mut cmd = std::process::Command::new("curl");
    cmd.arg("-sS")
        .arg("-L")
        .arg("-o")
        .arg(&out_path)
        .arg("-w")
        .arg("%{http_code}")
        .arg("-G")
        .arg(url);
    for (k, v) in query {
        cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    cmd.arg("-H")
        .arg(format!("Authorization: Bearer {token}"))
        .arg("-H")
        .arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H")
        .arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H")
        .arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"));

    let out = cmd
        .output()
        .map_err(|e| curl_err(format!("curl (GET): {e}")))?;
    if !out.status.success() {
        std::fs::remove_file(&out_path).ok();
        return Err(curl_err(format!(
            "curl (GET) process failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let code_str = String::from_utf8_lossy(&out.stdout);
    let http_code: u16 = code_str
        .trim()
        .parse()
        .map_err(|e| curl_err(format!("curl: invalid HTTP status {code_str:?}: {e}")))?;
    let body = std::fs::read_to_string(&out_path).map_err(|e| curl_err(e.to_string()))?;
    std::fs::remove_file(&out_path).ok();
    if !(200..300).contains(&http_code) {
        return Err(curl_err(format!(
            "GitHub API GET {url} returned HTTP {http_code}: {body}"
        )));
    }
    Ok(body)
}

/// HTTP PUT with a JSON body string (see [`curl_github_patch_json_with_token`]).
pub fn curl_github_put_json_with_token(
    repo: &str,
    path: &str,
    body: &str,
    token: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "PUT", body, token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_token_is_refused_before_a_patch_reaches_curl() {
        // Given no credential at all
        let token = "";

        // When a PATCH is attempted with it
        let result = curl_github_patch_json_with_token(
            "owner/repo",
            "pulls/1",
            r#"{"base":"master"}"#,
            token,
        );

        // Then it is refused
        assert!(
            result.is_err(),
            "a PATCH must return Err when no GitHub token is supplied; got: {result:?}"
        );
    }

    #[test]
    fn a_whitespace_token_is_refused_before_a_post_reaches_curl() {
        // Given a token that is only whitespace
        let token = "  \n";

        // When a POST is attempted with it
        let result = curl_github_post_json_with_token(
            "owner/repo",
            "pulls",
            r#"{"title":"x","head":"y","base":"master"}"#,
            token,
        );

        // Then it is refused
        assert!(
            result.is_err(),
            "a POST must return Err when the GitHub token is blank; got: {result:?}"
        );
    }

    #[test]
    fn a_blank_token_is_refused_before_a_get_reaches_curl() {
        // Given no credential at all
        let token = "";

        // When a GET is attempted with it
        let result = curl_github_get_json_with_token("owner/repo", "pulls", &[], token);

        // Then it is refused
        assert!(
            result.is_err(),
            "a GET must return Err when no GitHub token is supplied; got: {result:?}"
        );
    }
}
