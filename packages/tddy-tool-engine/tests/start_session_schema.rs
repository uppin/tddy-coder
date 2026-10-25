//! What `StartSession`'s schema tells the calling agent, and the request its arguments become
//! (`#start-parity` 2/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_core::cli_harness::{cli_harness_requires_model, cli_harnesses};
use tddy_service::proto::session::StartSessionRequest;
use tddy_tool_engine::start_session_args::{
    start_session_schema, HostChoice, StartChoices, StartContext, StartSessionArgs,
};

fn choices() -> StartChoices {
    StartChoices {
        hosts: vec![
            HostChoice {
                daemon_instance_id: "host-a".to_string(),
                label: "Host A".to_string(),
            },
            HostChoice {
                daemon_instance_id: "host-b".to_string(),
                label: "Host B".to_string(),
            },
        ],
        recipes: vec![
            "tdd".to_string(),
            "bugfix".to_string(),
            "pr-stack".to_string(),
        ],
    }
}

fn schema() -> Value {
    start_session_schema(&choices())
}

fn enum_of(property: &str) -> Vec<String> {
    serde_json::from_value(schema()["properties"][property]["enum"].clone())
        .unwrap_or_else(|_| panic!("{property} carries an enum"))
}

/// The values of `discriminant` whose `if`/`then` branch makes `field` required.
fn values_requiring(discriminant: &str, field: &str) -> BTreeSet<String> {
    schema()["allOf"]
        .as_array()
        .expect("the schema states its conditional requirements in allOf")
        .iter()
        .filter(|rule| {
            rule["then"]["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|r| r == field))
        })
        .filter_map(|rule| rule["if"]["properties"][discriminant]["const"].as_str())
        .map(str::to_string)
        .collect()
}

#[test]
fn session_type_offers_tool_workspace_and_every_registered_cli_harness() {
    // Given
    let mut expected = vec!["tool".to_string(), "workspace".to_string()];
    expected.extend(cli_harnesses().iter().map(|h| h.session_type.to_string()));

    // Then
    assert_eq!(enum_of("session_type"), expected);
}

#[test]
fn the_host_is_this_host_or_any_eligible_host() {
    assert_eq!(enum_of("daemon_instance_id"), vec!["", "host-a", "host-b"]);
}

#[test]
fn the_recipe_is_one_the_daemon_supports_or_none() {
    assert_eq!(enum_of("recipe"), vec!["", "tdd", "bugfix", "pr-stack"]);
}

#[test]
fn permission_mode_and_branch_intent_are_their_closed_lists() {
    assert_eq!(
        enum_of("permission_mode"),
        vec![
            "",
            "auto",
            "default",
            "acceptEdits",
            "plan",
            "bypassPermissions"
        ]
    );
    assert_eq!(
        enum_of("branch_worktree_intent"),
        vec!["", "new_branch_from_base", "work_on_selected_branch"]
    );
}

#[test]
fn model_is_required_exactly_for_the_harnesses_that_need_one() {
    // Given
    let expected: BTreeSet<String> = cli_harnesses()
        .iter()
        .map(|h| h.session_type)
        .filter(|t| cli_harness_requires_model(t))
        .map(str::to_string)
        .collect();

    // Then
    assert_eq!(values_requiring("session_type", "model"), expected);
}

#[test]
fn working_on_a_selected_branch_requires_naming_it() {
    assert_eq!(
        values_requiring("branch_worktree_intent", "selected_branch_to_work_on"),
        BTreeSet::from(["work_on_selected_branch".to_string()])
    );
}

#[test]
fn every_argument_is_described_and_only_session_type_is_always_required() {
    // When
    let schema = schema();

    // Then
    let properties = schema["properties"].as_object().expect("properties");
    assert!(
        properties.len() > 20,
        "the schema offers {} arguments",
        properties.len()
    );
    for (name, property) in properties {
        let description = property["description"].as_str().unwrap_or_default();
        assert!(!description.trim().is_empty(), "{name} has no description");
    }
    assert_eq!(schema["required"], serde_json::json!(["session_type"]));
    assert_eq!(schema["additionalProperties"], false);
}

fn a_context() -> StartContext {
    StartContext {
        session_token: "token-1".to_string(),
        project_id: "project-1".to_string(),
        orchestrator_session_id: "home-1".to_string(),
        orchestrator_daemon_instance_id: "host-a".to_string(),
    }
}

#[test]
fn every_argument_reaches_the_request_under_its_own_name() {
    // Given arguments with every field set to a value of its own
    let args: StartSessionArgs = serde_json::from_value(serde_json::json!({
        "session_type": "claude-cli",
        "daemon_instance_id": "host-b",
        "tool_path": "/usr/bin/tddy-coder",
        "agent": "claude",
        "recipe": "bugfix",
        "model": "opus",
        "provider_account_id": "account-1",
        "provider_built_in": true,
        "branch_worktree_intent": "new_branch_from_base",
        "new_branch_name": "feature/x",
        "selected_integration_base_ref": "origin/main",
        "selected_branch_to_work_on": "feature/y",
        "create_remote_branch": true,
        "initial_prompt": "do the thing",
        "permission_mode": "plan",
        "dangerously_skip_permissions": true,
        "stack_parent": "parent-1",
        "stack_parent_daemon_instance_id": "host-c",
        "stack_node_id": "node-1",
        "pr_stack_base_session_id": "base-1",
        "sandbox": true,
        "managed_codebase": true,
        "sandboxed_codebase": true,
        "codebase_daemon_instance_id": "host-d",
        "ssh_config_host": "builder",
        "semantic_index": true,
        "specialized_agents": ["fastcontext"],
        "attachments": [],
    }))
    .expect("every argument parses");

    // When
    let request = args.into_request(a_context());

    // Then
    assert_eq!(
        request,
        StartSessionRequest {
            session_token: "token-1".to_string(),
            project_id: "project-1".to_string(),
            on_branch_conflict: "reject".to_string(),
            orchestrator_session_id: "home-1".to_string(),
            orchestrator_daemon_instance_id: "host-a".to_string(),
            session_type: "claude-cli".to_string(),
            daemon_instance_id: "host-b".to_string(),
            tool_path: "/usr/bin/tddy-coder".to_string(),
            agent: "claude".to_string(),
            recipe: "bugfix".to_string(),
            model: "opus".to_string(),
            provider_account_id: "account-1".to_string(),
            provider_built_in: true,
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: "feature/x".to_string(),
            selected_integration_base_ref: "origin/main".to_string(),
            selected_branch_to_work_on: "feature/y".to_string(),
            create_remote_branch: true,
            initial_prompt: "do the thing".to_string(),
            permission_mode: "plan".to_string(),
            dangerously_skip_permissions: true,
            stack_parent: "parent-1".to_string(),
            stack_parent_daemon_instance_id: "host-c".to_string(),
            stack_node_id: "node-1".to_string(),
            pr_stack_base_session_id: "base-1".to_string(),
            sandbox: true,
            managed_codebase: true,
            sandboxed_codebase: true,
            codebase_daemon_instance_id: "host-d".to_string(),
            ssh_config_host: "builder".to_string(),
            semantic_index: true,
            specialized_agents: vec!["fastcontext".to_string()],
            ..Default::default()
        }
    );
}

#[test]
fn a_host_document_attachment_reaches_the_request_as_one() {
    // Given
    let args: StartSessionArgs = serde_json::from_value(serde_json::json!({
        "session_type": "workspace",
        "attachments": [{
            "basename": "brief.md",
            "daemon_instance_id": "host-a",
            "scope": "HOST_DOCUMENT_SCOPE_PROJECT_REPO",
            "project_id": "project-1",
            "relative_path": "docs/brief.md",
        }],
    }))
    .expect("an attachment parses");

    // When
    let request = args.into_request(a_context());

    // Then
    assert_eq!(request.attachments.len(), 1);
    let attachment = &request.attachments[0];
    assert_eq!(attachment.basename, "brief.md");
    let Some(tddy_service::proto::session::session_attachment::Source::HostDocument(document)) =
        &attachment.source
    else {
        panic!("the attachment is a host document: {attachment:?}");
    };
    assert_eq!(document.daemon_instance_id, "host-a");
    assert_eq!(document.project_id, "project-1");
    assert_eq!(document.relative_path, "docs/brief.md");
    assert_eq!(
        document.scope,
        tddy_service::proto::types::HostDocumentScope::ProjectRepo as i32
    );
}

#[test]
fn an_argument_the_tool_does_not_offer_is_refused_by_name() {
    // When
    let refused = serde_json::from_value::<StartSessionArgs>(serde_json::json!({
        "session_type": "workspace",
        "session_role": "project-home",
    }))
    .expect_err("a withheld field is not an argument");

    // Then
    assert!(refused.to_string().contains("session_role"), "{refused}");
}
