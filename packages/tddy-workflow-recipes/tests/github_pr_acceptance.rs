//! PRD Testing Plan: GitHub PR tools acceptance (auth, REST payload shape, MCP discovery).

use serde_json::json;
use tddy_workflow_recipes::github_pr::{
    create_pull_request, registered_github_pr_mcp_tool_names, update_pull_request,
    CreatePullRequestParams, MockGithubTransport, UpdatePullRequestParams,
    GITHUB_CREATE_PULL_REQUEST_MCP_NAME, GITHUB_UPDATE_PULL_REQUEST_MCP_NAME,
};

#[test]
fn github_tools_reject_when_token_missing() {
    // Given no token is passed
    let mut transport = MockGithubTransport::new();
    let params = CreatePullRequestParams {
        owner: "o".into(),
        repo: "r".into(),
        title: "t".into(),
        head: "feat".into(),
        base: "main".into(),
        body: "b".into(),
    };

    // When
    let result = create_pull_request(&mut transport, &params, "");

    // Then
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
fn github_tools_create_pr_sends_expected_rest_payload() {
    // Given
    let mut transport = MockGithubTransport::new();
    let params = CreatePullRequestParams {
        owner: "acme".into(),
        repo: "demo".into(),
        title: "Add feature".into(),
        head: "feature/foo".into(),
        base: "main".into(),
        body: "Body text".into(),
    };

    // When
    create_pull_request(&mut transport, &params, "ghp_testtoken_not_real")
        .expect("create PR with token");

    // Then
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
fn github_tools_update_pr_sends_expected_rest_payload() {
    // Given
    let mut transport = MockGithubTransport::new();
    let params = UpdatePullRequestParams {
        owner: "acme".into(),
        repo: "demo".into(),
        pull_number: 42,
        title: Some("New title".into()),
        body: None,
        draft: Some(true),
    };

    // When
    update_pull_request(&mut transport, &params, "ghp_testtoken_not_real")
        .expect("update PR with token");

    // Then
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

#[test]
fn mcp_server_lists_github_pr_tools() {
    // When / Then
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
