//! PRD Testing Plan: GitHub PR tools acceptance (auth, REST payload shape, MCP discovery).

use serde_json::json;
use serial_test::serial;
use tddy_tools::github_pr::{
    create_pull_request, github_token_from_env, registered_github_pr_mcp_tool_names,
    update_pull_request, CreatePullRequestParams, MockGithubTransport, UpdatePullRequestParams,
    GITHUB_CREATE_PULL_REQUEST_MCP_NAME, GITHUB_UPDATE_PULL_REQUEST_MCP_NAME,
};

struct EnvUnsetGithubTokens {
    had_gh: bool,
    had_github: bool,
    previous_gh: Option<String>,
    previous_github: Option<String>,
}

impl EnvUnsetGithubTokens {
    fn new() -> Self {
        let previous_gh = std::env::var("GH_TOKEN").ok();
        let previous_github = std::env::var("GITHUB_TOKEN").ok();
        let had_gh = previous_gh.is_some();
        let had_github = previous_github.is_some();
        std::env::remove_var("GH_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");
        Self {
            had_gh,
            had_github,
            previous_gh,
            previous_github,
        }
    }
}

impl Drop for EnvUnsetGithubTokens {
    fn drop(&mut self) {
        if self.had_github {
            if let Some(ref v) = self.previous_github {
                std::env::set_var("GITHUB_TOKEN", v);
            }
        } else {
            std::env::remove_var("GITHUB_TOKEN");
        }
        if self.had_gh {
            if let Some(ref v) = self.previous_gh {
                std::env::set_var("GH_TOKEN", v);
            }
        } else {
            std::env::remove_var("GH_TOKEN");
        }
    }
}

#[test]
#[serial]
fn github_tools_reject_when_token_missing() {
    let _env = EnvUnsetGithubTokens::new();
    assert!(
        github_token_from_env().is_none(),
        "test requires both GITHUB_TOKEN and GH_TOKEN unset"
    );

    let mut transport = MockGithubTransport::new();
    let params = CreatePullRequestParams {
        owner: "o".into(),
        repo: "r".into(),
        title: "t".into(),
        head: "feat".into(),
        base: "main".into(),
        body: "b".into(),
    };

    let result = create_pull_request(&mut transport, &params);
    assert!(
        result.is_err(),
        "expected authentication error when token missing, got: {result:?}"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("authentication"),
        "expected authentication-required style error, got: {err}"
    );
    assert!(
        transport.requests.is_empty(),
        "must not record HTTP when unauthenticated; got {:?}",
        transport.requests
    );
}

#[test]
#[serial]
fn github_tools_create_pr_sends_expected_rest_payload() {
    std::env::set_var("GITHUB_TOKEN", "ghp_testtoken_not_real");
    let _clear = ClearEnvOnDrop;
    let mut transport = MockGithubTransport::new();
    let params = CreatePullRequestParams {
        owner: "acme".into(),
        repo: "demo".into(),
        title: "Add feature".into(),
        head: "feature/foo".into(),
        base: "main".into(),
        body: "Body text".into(),
    };

    create_pull_request(&mut transport, &params).expect("create PR with token");

    assert_eq!(transport.requests.len(), 1);
    let req = &transport.requests[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/repos/acme/demo/pulls");
    assert_eq!(
        req.body,
        json!({
            "title": "Add feature",
            "head": "feature/foo",
            "base": "main",
            "body": "Body text"
        })
    );
}

#[test]
#[serial]
fn github_tools_update_pr_sends_expected_rest_payload() {
    std::env::set_var("GITHUB_TOKEN", "ghp_testtoken_not_real");
    let _clear = ClearEnvOnDrop;
    let mut transport = MockGithubTransport::new();
    let params = UpdatePullRequestParams {
        owner: "acme".into(),
        repo: "demo".into(),
        pull_number: 42,
        title: Some("New title".into()),
        body: None,
        draft: Some(true),
    };

    update_pull_request(&mut transport, &params).expect("update PR with token");

    assert_eq!(transport.requests.len(), 1);
    let req = &transport.requests[0];
    assert_eq!(req.method, "PATCH");
    assert_eq!(req.path, "/repos/acme/demo/pulls/42");
    assert_eq!(
        req.body,
        json!({
            "title": "New title",
            "draft": true
        })
    );
}

struct ClearEnvOnDrop;

impl Drop for ClearEnvOnDrop {
    fn drop(&mut self) {
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("GH_TOKEN");
    }
}

#[test]
#[serial]
fn mcp_server_lists_github_pr_tools() {
    let names = registered_github_pr_mcp_tool_names();
    assert!(
        names.contains(&GITHUB_CREATE_PULL_REQUEST_MCP_NAME),
        "MCP tool list must include {}, got {:?}",
        GITHUB_CREATE_PULL_REQUEST_MCP_NAME,
        names
    );
    assert!(
        names.contains(&GITHUB_UPDATE_PULL_REQUEST_MCP_NAME),
        "MCP tool list must include {}, got {:?}",
        GITHUB_UPDATE_PULL_REQUEST_MCP_NAME,
        names
    );
}
