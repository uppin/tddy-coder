//! Shared GitHub REST API constants and token resolution for **tddy-workflow-recipes** and consumers (e.g. **tddy-tools**).
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

/// Resolve GitHub token from the environment (`GITHUB_TOKEN` preferred, then `GH_TOKEN`).
#[must_use]
pub fn github_token_from_env() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("GH_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

/// True when a non-empty `GITHUB_TOKEN` or `GH_TOKEN` is set (prompt gating, merge-pr, etc.).
#[must_use]
pub fn github_env_token_present() -> bool {
    github_token_from_env().is_some()
}

fn github_api_url(repo: &str, path: &str) -> String {
    format!("https://api.github.com/repos/{repo}/{path}")
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
    let body_path = temp_github_path("tddy-gh-req-body");
    let out_path = temp_github_path("tddy-gh-resp");
    std::fs::write(&body_path, body.as_bytes()).map_err(|e| curl_err(e.to_string()))?;

    let out = std::process::Command::new("curl")
        .arg("-sS")
        .arg("-L")
        .arg("-o").arg(&out_path)
        .arg("-w").arg("%{http_code}")
        .arg("-X").arg(method)
        .arg("-H").arg(format!("Authorization: Bearer {token}"))
        .arg("-H").arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H").arg("Content-Type: application/json")
        .arg("-H").arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H").arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"))
        .arg("--data-binary").arg(format!("@{}", body_path.display()))
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

/// HTTP PATCH with a JSON body string, returns the response body.
/// Uses curl; token from `github_token_from_env()`.
pub fn curl_github_patch_json(
    repo: &str,
    path: &str,
    body: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let token = github_token_from_env().ok_or_else(|| {
        curl_err("curl_github_patch_json: no GitHub token set (GITHUB_TOKEN / GH_TOKEN)")
    })?;
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "PATCH", body, &token)
}

/// HTTP POST with a JSON body string, returns the response body.
/// Uses curl; token from `github_token_from_env()`.
pub fn curl_github_post_json(
    repo: &str,
    path: &str,
    body: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let token = github_token_from_env().ok_or_else(|| {
        curl_err("curl_github_post_json: no GitHub token set (GITHUB_TOKEN / GH_TOKEN)")
    })?;
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "POST", body, &token)
}

/// HTTP GET with query parameters, returns the response body.
pub fn curl_github_get_json(
    repo: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<String, tddy_core::WorkflowError> {
    let token = github_token_from_env().ok_or_else(|| {
        curl_err("curl_github_get_json: no GitHub token set (GITHUB_TOKEN / GH_TOKEN)")
    })?;
    let url = github_api_url(repo, path);
    let out_path = temp_github_path("tddy-gh-get");

    let mut cmd = std::process::Command::new("curl");
    cmd.arg("-sS")
        .arg("-L")
        .arg("-o").arg(&out_path)
        .arg("-w").arg("%{http_code}")
        .arg("-G")
        .arg(&url);
    for (k, v) in query {
        cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    cmd.arg("-H").arg(format!("Authorization: Bearer {token}"))
        .arg("-H").arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H").arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H").arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"));

    let out = cmd.output().map_err(|e| curl_err(format!("curl (GET): {e}")))?;
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

/// HTTP PUT with a JSON body string, returns the response body.
pub fn curl_github_put_json(
    repo: &str,
    path: &str,
    body: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let token = github_token_from_env().ok_or_else(|| {
        curl_err("curl_github_put_json: no GitHub token set (GITHUB_TOKEN / GH_TOKEN)")
    })?;
    let url = github_api_url(repo, path);
    run_curl_json_body(&url, "PUT", body, &token)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `curl_github_patch_json_returns_error_without_token` — when neither `GITHUB_TOKEN` nor
    /// `GH_TOKEN` is set, `curl_github_patch_json` must return `Err` (token required) rather than
    /// panicking or calling curl with an empty Authorization header.
    #[test]
    fn curl_github_patch_json_returns_error_without_token() {
        // Ensure the env vars are unset for this test.
        // Note: test binaries run in parallel; use serial_test if env mutation causes flakiness.
        let token_backup = (
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
        }

        let result = curl_github_patch_json("owner/repo", "pulls/1", r#"{"base":"master"}"#);

        // Restore
        if let Some(t) = token_backup.0 {
            unsafe { std::env::set_var("GITHUB_TOKEN", t) };
        }
        if let Some(t) = token_backup.1 {
            unsafe { std::env::set_var("GH_TOKEN", t) };
        }

        assert!(
            result.is_err(),
            "curl_github_patch_json must return Err when no GitHub token is set; got: {result:?}"
        );
    }

    /// `curl_github_post_json_returns_error_without_token` — same gate for POST.
    #[test]
    fn curl_github_post_json_returns_error_without_token() {
        let token_backup = (
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
        }

        let result = curl_github_post_json("owner/repo", "pulls", r#"{"title":"x","head":"y","base":"master"}"#);

        if let Some(t) = token_backup.0 {
            unsafe { std::env::set_var("GITHUB_TOKEN", t) };
        }
        if let Some(t) = token_backup.1 {
            unsafe { std::env::set_var("GH_TOKEN", t) };
        }

        assert!(
            result.is_err(),
            "curl_github_post_json must return Err when no GitHub token is set; got: {result:?}"
        );
    }
}
