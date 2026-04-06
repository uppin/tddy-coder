//! GitHub pull-request helpers and MCP registration surface for agents (GitHub REST).

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use serde_json::{json, Value};
pub use tddy_workflow_recipes::{
    github_token_from_env, GITHUB_ACCEPT, GITHUB_API_VERSION, USER_AGENT_TDDY_TOOLS,
};

/// User-Agent for GitHub API requests from **tddy-tools** (alias of [`USER_AGENT_TDDY_TOOLS`]).
pub const GITHUB_USER_AGENT: &str = USER_AGENT_TDDY_TOOLS;

/// Stable MCP tool names for GitHub PR operations (must match MCP registration).
pub const GITHUB_CREATE_PULL_REQUEST_MCP_NAME: &str = "github_create_pull_request";
pub const GITHUB_UPDATE_PULL_REQUEST_MCP_NAME: &str = "github_update_pull_request";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedHttpRequest {
    pub method: String,
    pub path: String,
    /// Captured HTTP headers (e.g. `Authorization`, `Accept`, `User-Agent`) for tests and debugging.
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Value,
}

/// Test-only transport that records GitHub REST calls without network I/O.
#[derive(Debug, Default)]
pub struct MockGithubTransport {
    pub requests: Vec<RecordedHttpRequest>,
}

impl MockGithubTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct CreatePullRequestParams {
    pub owner: String,
    pub repo: String,
    pub title: String,
    pub head: String,
    pub base: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct UpdatePullRequestParams {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubPrError {
    /// Missing or empty `GITHUB_TOKEN` / `GH_TOKEN`.
    AuthenticationRequired,
    /// Transport, HTTP, or API error (message must never include token material).
    Rest(String),
}

impl std::fmt::Display for GithubPrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GithubPrError::AuthenticationRequired => {
                write!(
                    f,
                    "GitHub authentication required: set GITHUB_TOKEN or GH_TOKEN"
                )
            }
            GithubPrError::Rest(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for GithubPrError {}

/// Headers for GitHub REST calls (Bearer, Accept, User-Agent, API version).
#[must_use]
pub fn github_rest_headers(token: &str) -> BTreeMap<String, String> {
    let mut h = BTreeMap::new();
    h.insert("Authorization".into(), format!("Bearer {}", token.trim()));
    h.insert("Accept".into(), GITHUB_ACCEPT.into());
    h.insert("User-Agent".into(), GITHUB_USER_AGENT.into());
    h.insert("X-GitHub-Api-Version".into(), GITHUB_API_VERSION.into());
    h
}

fn build_create_pull_request_json(params: &CreatePullRequestParams) -> Value {
    json!({
        "title": params.title,
        "head": params.head,
        "base": params.base,
        "body": params.body,
    })
}

fn build_update_pull_request_json(params: &UpdatePullRequestParams) -> Value {
    let mut m = serde_json::Map::new();
    if let Some(ref t) = params.title {
        m.insert("title".into(), json!(t));
    }
    if let Some(ref b) = params.body {
        m.insert("body".into(), json!(b));
    }
    if let Some(d) = params.draft {
        m.insert("draft".into(), json!(d));
    }
    Value::Object(m)
}

/// Names of GitHub PR MCP tools exposed by `tddy-tools --mcp`.
pub fn registered_github_pr_mcp_tool_names() -> Vec<&'static str> {
    log::debug!(
        target: "tddy_tools::github_pr",
        "registered_github_pr_mcp_tool_names: {} + {}",
        GITHUB_CREATE_PULL_REQUEST_MCP_NAME,
        GITHUB_UPDATE_PULL_REQUEST_MCP_NAME
    );
    vec![
        GITHUB_CREATE_PULL_REQUEST_MCP_NAME,
        GITHUB_UPDATE_PULL_REQUEST_MCP_NAME,
    ]
}

/// Create a pull request via the GitHub REST API (recorded on `MockGithubTransport` in tests).
pub fn create_pull_request(
    transport: &mut MockGithubTransport,
    params: &CreatePullRequestParams,
) -> Result<u64, GithubPrError> {
    log::debug!(
        target: "tddy_tools::github_pr",
        "create_pull_request (mock transport) owner={} repo={}",
        params.owner,
        params.repo
    );
    let Some(token) = github_token_from_env() else {
        log::info!(
            target: "tddy_tools::github_pr",
            "create_pull_request: no GITHUB_TOKEN/GH_TOKEN — rejecting before HTTP"
        );
        return Err(GithubPrError::AuthenticationRequired);
    };

    let headers = github_rest_headers(&token);
    let body = build_create_pull_request_json(params);
    log::debug!(
        target: "tddy_tools::github_pr",
        "create_pull_request: recording POST /repos/{}/{}/pulls header_keys={:?}",
        params.owner,
        params.repo,
        headers.keys().collect::<Vec<_>>()
    );

    transport.requests.push(RecordedHttpRequest {
        method: "POST".into(),
        path: format!("/repos/{}/{}/pulls", params.owner, params.repo),
        headers,
        body,
    });
    Ok(1)
}

/// Update an existing pull request (metadata).
pub fn update_pull_request(
    transport: &mut MockGithubTransport,
    params: &UpdatePullRequestParams,
) -> Result<(), GithubPrError> {
    log::debug!(
        target: "tddy_tools::github_pr",
        "update_pull_request (mock transport) owner={} repo={} pr={}",
        params.owner,
        params.repo,
        params.pull_number
    );
    let Some(token) = github_token_from_env() else {
        log::info!(
            target: "tddy_tools::github_pr",
            "update_pull_request: no GITHUB_TOKEN/GH_TOKEN — rejecting before HTTP"
        );
        return Err(GithubPrError::AuthenticationRequired);
    };

    let headers = github_rest_headers(&token);
    let body = build_update_pull_request_json(params);
    log::debug!(
        target: "tddy_tools::github_pr",
        "update_pull_request: recording PATCH pulls/{} header_keys={:?}",
        params.pull_number,
        headers.keys().collect::<Vec<_>>()
    );

    transport.requests.push(RecordedHttpRequest {
        method: "PATCH".into(),
        path: format!(
            "/repos/{}/{}/pulls/{}",
            params.owner, params.repo, params.pull_number
        ),
        headers,
        body,
    });
    Ok(())
}

/// Create a pull request over the network (used by MCP). Requires `curl` on `PATH`.
pub fn create_pull_request_via_rest_api(
    params: &CreatePullRequestParams,
) -> Result<u64, GithubPrError> {
    log::info!(
        target: "tddy_tools::github_pr",
        "create_pull_request_via_rest_api owner={} repo={}",
        params.owner,
        params.repo
    );
    let token = github_token_from_env().ok_or(GithubPrError::AuthenticationRequired)?;
    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls",
        params.owner, params.repo
    );
    let body = build_create_pull_request_json(params);
    let (status, raw) = curl_github_json("POST", &url, &body, &token)?;
    if !(200..300).contains(&status) {
        log::debug!(
            target: "tddy_tools::github_pr",
            "create_pull_request_via_rest_api: HTTP {} body_len={}",
            status,
            raw.len()
        );
        return Err(GithubPrError::Rest(format!(
            "GitHub API create pull request failed with HTTP {status}"
        )));
    }
    let v: Value = serde_json::from_slice(&raw).map_err(|e| {
        GithubPrError::Rest(format!(
            "failed to parse GitHub create pull response JSON: {e}"
        ))
    })?;
    let number = v
        .get("number")
        .and_then(|n| n.as_u64())
        .ok_or_else(|| GithubPrError::Rest("GitHub create pull response missing number".into()))?;
    log::debug!(
        target: "tddy_tools::github_pr",
        "create_pull_request_via_rest_api: created pull #{}",
        number
    );
    Ok(number)
}

/// Update a pull request over the network (used by MCP). Requires `curl` on `PATH`.
pub fn update_pull_request_via_rest_api(
    params: &UpdatePullRequestParams,
) -> Result<(), GithubPrError> {
    log::info!(
        target: "tddy_tools::github_pr",
        "update_pull_request_via_rest_api owner={} repo={} pr={}",
        params.owner,
        params.repo,
        params.pull_number
    );
    let token = github_token_from_env().ok_or(GithubPrError::AuthenticationRequired)?;
    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}",
        params.owner, params.repo, params.pull_number
    );
    let body = build_update_pull_request_json(params);
    let (status, raw) = curl_github_json("PATCH", &url, &body, &token)?;
    if !(200..300).contains(&status) {
        log::debug!(
            target: "tddy_tools::github_pr",
            "update_pull_request_via_rest_api: HTTP {} body_len={}",
            status,
            raw.len()
        );
        return Err(GithubPrError::Rest(format!(
            "GitHub API update pull request failed with HTTP {status}"
        )));
    }
    log::debug!(
        target: "tddy_tools::github_pr",
        "update_pull_request_via_rest_api: success pr={}",
        params.pull_number
    );
    let _ = raw;
    Ok(())
}

fn curl_github_json(
    method: &str,
    url: &str,
    json_body: &Value,
    token: &str,
) -> Result<(u16, Vec<u8>), GithubPrError> {
    let dir = tempfile::tempdir().map_err(|e| GithubPrError::Rest(e.to_string()))?;
    let body_path = dir.path().join("body.json");
    let out_path = dir.path().join("out.json");
    fs::write(
        &body_path,
        serde_json::to_vec(json_body).map_err(|e| GithubPrError::Rest(e.to_string()))?,
    )
    .map_err(|e| GithubPrError::Rest(e.to_string()))?;

    let output = Command::new("curl")
        .arg("-sS")
        .arg("-L")
        .arg("-o")
        .arg(&out_path)
        .arg("-w")
        .arg("%{http_code}")
        .arg("-X")
        .arg(method)
        .arg("-H")
        .arg(format!("Authorization: Bearer {}", token.trim()))
        .arg("-H")
        .arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(format!("User-Agent: {GITHUB_USER_AGENT}"))
        .arg("-H")
        .arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"))
        .arg("--data-binary")
        .arg(format!("@{}", body_path.display()))
        .arg(url)
        .output()
        .map_err(|e| GithubPrError::Rest(format!("curl ({method} {url}): {e}")))?;

    if !output.status.success() {
        return Err(GithubPrError::Rest(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let code_str = String::from_utf8_lossy(&output.stdout);
    let status: u16 = code_str.trim().parse().map_err(|e| {
        GithubPrError::Rest(format!(
            "curl: invalid HTTP status {:?}: {e}",
            code_str.trim()
        ))
    })?;

    let bytes = fs::read(&out_path).map_err(|e| GithubPrError::Rest(e.to_string()))?;
    Ok((status, bytes))
}

#[cfg(test)]
mod red_unit_tests {
    use super::*;
    use serde_json::json;
    use serial_test::serial;

    #[test]
    #[serial]
    fn github_rest_recorded_request_includes_bearer_and_accept_headers() {
        std::env::set_var("GITHUB_TOKEN", "ghp_unit_test_token");
        let _clear = ClearGithubEnvOnDrop;
        let mut transport = MockGithubTransport::new();
        let params = CreatePullRequestParams {
            owner: "o".into(),
            repo: "r".into(),
            title: "t".into(),
            head: "h".into(),
            base: "main".into(),
            body: "b".into(),
        };
        create_pull_request(&mut transport, &params).expect("authenticated create");
        let req = transport.requests.first().expect("one request");
        let auth = req
            .headers
            .get("Authorization")
            .map(String::as_str)
            .unwrap_or("");
        assert!(
            auth.starts_with("Bearer "),
            "expected Bearer token header once wired; headers={:?}",
            req.headers
        );
        assert_eq!(
            req.headers.get("Accept").map(String::as_str),
            Some("application/vnd.github+json")
        );
    }

    struct ClearGithubEnvOnDrop;

    impl Drop for ClearGithubEnvOnDrop {
        fn drop(&mut self) {
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
        }
    }

    #[test]
    fn registered_github_pr_mcp_tool_names_includes_create_and_update() {
        let names = registered_github_pr_mcp_tool_names();
        assert_eq!(
            names,
            vec![
                GITHUB_CREATE_PULL_REQUEST_MCP_NAME,
                GITHUB_UPDATE_PULL_REQUEST_MCP_NAME
            ]
        );
    }

    #[test]
    #[serial]
    fn create_pull_request_rejects_without_github_token() {
        struct RestoreEnv {
            github: Option<String>,
            gh: Option<String>,
        }

        impl Drop for RestoreEnv {
            fn drop(&mut self) {
                match &self.github {
                    Some(v) => std::env::set_var("GITHUB_TOKEN", v),
                    None => std::env::remove_var("GITHUB_TOKEN"),
                }
                match &self.gh {
                    Some(v) => std::env::set_var("GH_TOKEN", v),
                    None => std::env::remove_var("GH_TOKEN"),
                }
            }
        }

        let _restore = RestoreEnv {
            github: std::env::var("GITHUB_TOKEN").ok(),
            gh: std::env::var("GH_TOKEN").ok(),
        };
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("GH_TOKEN");

        let mut transport = MockGithubTransport::new();
        let params = CreatePullRequestParams {
            owner: "o".into(),
            repo: "r".into(),
            title: "t".into(),
            head: "h".into(),
            base: "b".into(),
            body: "x".into(),
        };
        let result = create_pull_request(&mut transport, &params);
        assert!(result.is_err(), "expected auth error, got {result:?}");
        assert!(matches!(result, Err(GithubPrError::AuthenticationRequired)));
        assert!(transport.requests.is_empty());
    }

    #[test]
    #[serial]
    fn create_pull_request_json_body_matches_github_pulls_contract() {
        std::env::set_var("GITHUB_TOKEN", "ghp_unit");
        let _clear = ClearGithubEnvOnDrop;
        let mut transport = MockGithubTransport::new();
        let params = CreatePullRequestParams {
            owner: "a".into(),
            repo: "b".into(),
            title: "T".into(),
            head: "feat".into(),
            base: "main".into(),
            body: "BB".into(),
        };
        create_pull_request(&mut transport, &params).unwrap();
        assert_eq!(
            transport.requests[0].body,
            json!({
                "title": "T",
                "head": "feat",
                "base": "main",
                "body": "BB"
            })
        );
    }
}
