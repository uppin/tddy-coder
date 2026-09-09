use super::*;

const AGENT_INSTANCE_ID: &str = "laptop-a";
const AGENT_SESSION_ID: &str = "agent-session-1";
const CODEBASE_SESSION_ID: &str = "codebase-session-1";

/// The split start an operator sends: a managed-codebase claude-cli session whose codebase the
/// operator placed on another host.
fn a_split_start_request() -> StartSessionRequest {
    StartSessionRequest {
        session_type: "claude-cli".to_string(),
        model: "claude-opus-4-8".to_string(),
        project_id: "proj-1".to_string(),
        managed_codebase: true,
        sandbox: true,
        codebase_daemon_instance_id: "workstation-b".to_string(),
        daemon_instance_id: AGENT_INSTANCE_ID.to_string(),
        ..Default::default()
    }
}

fn forwarded(req: &StartSessionRequest) -> StartSessionRequest {
    workspace_start_request(
        req,
        AGENT_INSTANCE_ID,
        AGENT_SESSION_ID,
        CODEBASE_SESSION_ID,
    )
    .expect("a well-formed split start must build a workspace request")
}

/// A bare name means "the agent host's agent". The codebase host would read it as its own, so it
/// travels qualified.
#[test]
fn a_bare_agent_reference_is_qualified_with_the_agent_hosts_instance_id() {
    // Given
    let req = StartSessionRequest {
        specialized_agents: vec!["reviewer".to_string()],
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(
        forwarded.specialized_agents,
        vec!["reviewer@laptop-a".to_string()]
    );
}

/// An agent the operator placed on a third host keeps that host: qualification names the default
/// owner, it does not move an agent that already has one.
#[test]
fn an_already_qualified_agent_reference_keeps_the_host_it_names() {
    // Given
    let req = StartSessionRequest {
        specialized_agents: vec!["explorer@workstation-c".to_string()],
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(
        forwarded.specialized_agents,
        vec!["explorer@workstation-c".to_string()]
    );
}

/// Order is the operator's, and it is the order the roster keeps.
#[test]
fn agent_references_are_forwarded_in_the_order_they_were_asked_for() {
    // Given
    let req = StartSessionRequest {
        specialized_agents: vec!["explorer@workstation-c".to_string(), "reviewer".to_string()],
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(
        forwarded.specialized_agents,
        vec![
            "explorer@workstation-c".to_string(),
            "reviewer@laptop-a".to_string(),
        ]
    );
}

/// A reference no host can be read out of is a request error here, before a worktree exists on
/// the peer — not a string forwarded for the peer to choke on.
#[test]
fn an_ambiguous_agent_reference_is_refused_as_a_request_error() {
    // Given
    let req = StartSessionRequest {
        specialized_agents: vec!["explorer@a@b".to_string()],
        ..a_split_start_request()
    };

    // When
    let result = workspace_start_request(
        &req,
        AGENT_INSTANCE_ID,
        AGENT_SESSION_ID,
        CODEBASE_SESSION_ID,
    );

    // Then
    let status = result.expect_err("an ambiguous reference must be refused");
    assert_eq!(status.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        status.message().contains("explorer@a@b"),
        "the refusal must name the reference it refused: {}",
        status.message()
    );
}

/// The peer runs this session itself. Both placement fields are cleared, so it cannot route the
/// request onward and split the session again.
#[test]
fn the_forwarded_request_names_no_placement_of_its_own() {
    // Given
    let req = a_split_start_request();

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(forwarded.session_type, "workspace".to_string());
    assert_eq!(forwarded.daemon_instance_id, String::new());
    assert_eq!(forwarded.codebase_daemon_instance_id, String::new());
}

/// The id this daemon minted travels with the request, so the worktree can be torn down by name
/// even when the forward never answers.
#[test]
fn the_forwarded_request_carries_the_session_id_this_daemon_minted() {
    // Given
    let req = a_split_start_request();

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(
        forwarded.requested_session_id,
        CODEBASE_SESSION_ID.to_string()
    );
}

/// The workspace session persists the back-pointer: it is what tells a host running no agent of
/// its own which agent, on which daemon, a withdrawal on this checkout is enforced against.
#[test]
fn the_forwarded_request_points_the_workspace_session_back_at_the_agent() {
    // Given
    let req = a_split_start_request();

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(
        forwarded.split_agent,
        Some(SplitAgentPlacement {
            session_id: AGENT_SESSION_ID.to_string(),
            agent_daemon_instance_id: AGENT_INSTANCE_ID.to_string(),
        })
    );
}

/// Attachments are read by the agent and by the browser's Docs listing, both of which act on the
/// agent half. Sending them on would put an unread copy on the codebase host, inside the
/// forward's deadline.
#[test]
fn attachments_stay_on_the_agent_half() {
    // Given
    let req = StartSessionRequest {
        attachments: vec![SessionAttachment {
            basename: "brief.md".to_string(),
            ..Default::default()
        }],
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert_eq!(forwarded.attachments, Vec::new());
}

/// The index indexes a worktree, and on a split placement the worktree is on the host this
/// request is going to — so the flag travels with it.
#[test]
fn a_requested_semantic_index_is_asked_of_the_host_holding_the_worktree() {
    // Given
    let req = StartSessionRequest {
        semantic_index: true,
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert!(forwarded.semantic_index);
}

/// A sandbox on a split placement confines the codebase half, so the flag travels with the
/// request to the host holding the checkout — `run_exec_tool_locally` on that host reads it
/// back off the workspace metadata to route through the jail. The forward is `..req.clone()`,
/// so this pins the contract against a future refactor that drops the field: a silent drop
/// would leave the codebase half unsandboxed with nothing here saying it should have been.
#[test]
fn a_requested_sandbox_is_forwarded_to_the_host_holding_the_worktree() {
    // Given — `a_split_start_request` already carries `sandbox: true`
    let req = a_split_start_request();

    // When
    let forwarded = forwarded(&req);

    // Then
    assert!(
        forwarded.sandbox,
        "the sandbox flag must reach the codebase host so its workspace half is confined"
    );
}

/// The control: a split placement that declines a sandbox forwards no sandbox, so the
/// codebase host's workspace session stays recognisably unsandboxed — the same shape every
/// split session had before this feature.
#[test]
fn an_unsandboxed_split_start_forwards_no_sandbox() {
    // Given
    let req = StartSessionRequest {
        sandbox: false,
        ..a_split_start_request()
    };

    // When
    let forwarded = forwarded(&req);

    // Then
    assert!(
        !forwarded.sandbox,
        "an unsandboxed split start must not impose a sandbox on the codebase host"
    );
}
