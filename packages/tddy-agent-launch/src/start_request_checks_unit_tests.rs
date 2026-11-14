//! A session start forwarded to a peer carries this host's GitHub source for the project, so the
//! peer acts the same way even if its own copy of the project was never set.

use std::collections::HashMap;
use std::sync::Arc;

use tddy_projects::project_storage::ProjectData;
use tddy_service::proto::session::{GithubSource, StartSessionRequest};

use crate::spec_launch::HostCodingAgents;
use crate::start_request_checks::{
    forwarded_start_request, record_requested_github_source, refuse_a_conflicting_tmux_run_start,
    refuse_a_tool_path_the_daemon_does_not_allow, refuse_an_orchestrated_tool_start,
};

/// The built-in coding agents and session types, on a host whose binaries are never resolved.
fn the_builtin_coding_agents() -> HostCodingAgents {
    let no_overrides = tempfile::tempdir().expect("an empty tools dir");
    HostCodingAgents::new(
        tddy_coding_agent_specs::load_agent_registry(no_overrides.path())
            .expect("the built-in coding agents load"),
        Arc::new(tddy_actions::tool::ProcessToolHost),
    )
}

const PROJECT_ID: &str = "11111111-2222-4333-8444-555555555555";

fn a_project_row(local_gh: bool) -> ProjectData {
    ProjectData {
        project_id: PROJECT_ID.to_string(),
        name: "alpha".to_string(),
        git_url: "https://github.com/acme/alpha.git".to_string(),
        main_repo_path: "/home/dev/repos/alpha".to_string(),
        main_branch_ref: None,
        remote_name: None,
        host_repo_paths: HashMap::new(),
        accounts: Vec::new(),
        local_gh,
        home_session_id: None,
    }
}

fn a_start_naming(github_source: GithubSource) -> StartSessionRequest {
    StartSessionRequest {
        session_token: "session-token-for-ada".to_string(),
        project_id: PROJECT_ID.to_string(),
        daemon_instance_id: "build-box-2".to_string(),
        github_source: github_source as i32,
        ..Default::default()
    }
}

#[test]
fn a_forwarded_start_of_a_local_gh_project_names_local_gh() {
    // Given a start naming no source, of a project this host knows as Local gh
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(true)));

    // Then the peer is told Local gh
    assert_eq!(forwarded.github_source(), GithubSource::LocalGh);
}

#[test]
fn a_forwarded_start_of_a_vault_project_names_the_vault() {
    // Given a start naming no source, of a project this host knows as using the vault
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(false)));

    // Then the peer is told the vault
    assert_eq!(forwarded.github_source(), GithubSource::Vault);
}

#[test]
fn a_forwarded_start_that_already_names_a_source_keeps_it() {
    // Given a start already naming Local gh, of a project this host knows as using the vault
    let req = a_start_naming(GithubSource::LocalGh);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(false)));

    // Then the source it named is the one forwarded
    assert_eq!(forwarded.github_source(), GithubSource::LocalGh);
}

#[test]
fn a_forwarded_start_of_a_project_this_host_does_not_hold_names_no_source() {
    // Given a start naming no source, of a project this host has no row for
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, None);

    // Then the peer decides from its own row
    assert_eq!(forwarded.github_source(), GithubSource::Unspecified);
}

#[test]
fn a_forwarded_start_is_otherwise_the_request_as_received() {
    // Given a start naming no source, of a Local gh project
    let req = a_start_naming(GithubSource::Unspecified);

    // When it is forwarded
    let forwarded = forwarded_start_request(&req, Some(&a_project_row(true)));

    // Then everything but the source is unchanged
    assert_eq!(
        StartSessionRequest {
            github_source: req.github_source,
            ..forwarded
        },
        req
    );
}

#[test]
fn a_start_that_carried_a_source_records_it_in_the_sessions_github_source_file() {
    // Given a start naming Local gh and a session directory that does not exist yet
    let scratch = tempfile::tempdir().unwrap();
    let session_dir = scratch.path().join("session-1");
    let req = a_start_naming(GithubSource::LocalGh);

    // When the start's source is recorded
    record_requested_github_source(&req, &session_dir).unwrap();

    // Then the sidecar file exists and reads back as Local gh
    assert!(session_dir.join("github-source.yaml").is_file());
    assert_eq!(
        tddy_core::read_session_github_source(&session_dir).unwrap(),
        Some(tddy_core::SessionGithubSource::LocalGh)
    );
}

#[test]
fn a_start_that_named_no_source_writes_no_github_source_file() {
    // Given a start naming no source
    let scratch = tempfile::tempdir().unwrap();
    let req = a_start_naming(GithubSource::Unspecified);

    // When the start's source is recorded
    record_requested_github_source(&req, scratch.path()).unwrap();

    // Then nothing is written: resumes read this host's row again
    assert!(!scratch.path().join("github-source.yaml").exists());
}

fn an_orchestrated_start_of(session_type: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_type: session_type.to_string(),
        orchestrator_session_id: "home-1".to_string(),
        ..Default::default()
    }
}

#[test]
fn an_orchestrated_tool_start_is_refused_naming_the_tool_session_type() {
    // When — `tool` is declared by tddy-coder.yaml with the tddy_coder launch
    let refused = refuse_an_orchestrated_tool_start(
        &an_orchestrated_start_of("tool"),
        &the_builtin_coding_agents(),
    )
    .expect_err("a tool session cannot record its orchestrator");

    // Then
    assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
    assert!(refused.message.contains("\"tool\""), "{}", refused.message);
}

#[test]
fn an_orchestrated_start_of_a_session_the_daemon_records_is_accepted() {
    let agents = the_builtin_coding_agents();
    for session_type in ["workspace", "claude-cli"] {
        assert!(
            refuse_an_orchestrated_tool_start(&an_orchestrated_start_of(session_type), &agents)
                .is_ok(),
            "{session_type}"
        );
    }
}

#[test]
fn a_tool_start_with_no_orchestrator_is_accepted() {
    assert!(refuse_an_orchestrated_tool_start(
        &StartSessionRequest {
            session_type: "tool".to_string(),
            ..Default::default()
        },
        &the_builtin_coding_agents(),
    )
    .is_ok());
}

/// A daemon whose `allowed_tools` is the one dev build.
fn a_daemon_allowing_the_dev_build() -> tddy_daemon_kernel::config::DaemonConfig {
    serde_yaml::from_str("allowed_tools:\n  - path: target/debug/tddy-coder\n")
        .expect("a daemon config with allowed_tools")
}

fn a_tool_start_of(tool_path: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_type: "tool".to_string(),
        tool_path: tool_path.to_string(),
        ..Default::default()
    }
}

#[test]
fn a_tool_start_of_an_allowed_tool_path_is_accepted() {
    assert_eq!(
        refuse_a_tool_path_the_daemon_does_not_allow(
            &a_tool_start_of("target/debug/tddy-coder"),
            &a_daemon_allowing_the_dev_build(),
        )
        .map_err(|status| status.message),
        Ok(())
    );
}

#[test]
fn a_tool_start_of_an_empty_or_unlisted_tool_path_is_refused_naming_the_allowlist() {
    // When
    let refused: Vec<(tddy_rpc::Code, String)> = ["", "/bin/sh"]
        .into_iter()
        .map(|tool_path| {
            refuse_a_tool_path_the_daemon_does_not_allow(
                &a_tool_start_of(tool_path),
                &a_daemon_allowing_the_dev_build(),
            )
            .map(|()| (tddy_rpc::Code::Ok, String::new()))
            .unwrap_or_else(|status| (status.code, status.message))
        })
        .collect();

    // Then
    assert_eq!(
        refused,
        vec![
            (
                tddy_rpc::Code::InvalidArgument,
                "tool_path is required for a \"tool\" session; this daemon's allowed_tools are \
                 [\"target/debug/tddy-coder\"]"
                    .to_string()
            ),
            (
                tddy_rpc::Code::InvalidArgument,
                "tool_path \"/bin/sh\" is not in allowed_tools; this daemon's allowed_tools are \
                 [\"target/debug/tddy-coder\"]"
                    .to_string()
            ),
        ]
    );
}

fn a_tmux_run_claude_start() -> StartSessionRequest {
    StartSessionRequest {
        session_type: "claude-cli".to_string(),
        run_in_tmux: true,
        ..Default::default()
    }
}

#[test]
fn a_tmux_run_start_with_a_sandboxed_codebase_is_refused_naming_the_conflict() {
    // Given
    let req = StartSessionRequest {
        sandboxed_codebase: true,
        ..a_tmux_run_claude_start()
    };

    // When
    let refusal =
        refuse_a_conflicting_tmux_run_start(&req, Some(&the_builtin_coding_agents())).unwrap_err();

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        refusal.message().contains("sandboxed_codebase"),
        "{}",
        refusal.message()
    );
    assert!(
        refusal.message().contains("run_in_tmux"),
        "{}",
        refusal.message()
    );
}

#[test]
fn a_tmux_run_start_with_a_split_placement_is_refused_naming_the_conflict() {
    // Given
    let req = StartSessionRequest {
        codebase_daemon_instance_id: "build-box-2".to_string(),
        ..a_tmux_run_claude_start()
    };

    // When
    let refusal =
        refuse_a_conflicting_tmux_run_start(&req, Some(&the_builtin_coding_agents())).unwrap_err();

    // Then
    assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        refusal.message().contains("codebase_daemon_instance_id"),
        "{}",
        refusal.message()
    );
}

#[test]
fn a_tmux_run_start_on_a_plain_placement_is_not_refused() {
    assert!(refuse_a_conflicting_tmux_run_start(
        &a_tmux_run_claude_start(),
        Some(&the_builtin_coding_agents())
    )
    .is_ok());
}

#[test]
fn a_split_placement_without_tmux_is_not_refused_here() {
    // Given
    let req = StartSessionRequest {
        session_type: "claude-cli".to_string(),
        codebase_daemon_instance_id: "build-box-2".to_string(),
        ..Default::default()
    };

    // Then
    assert!(refuse_a_conflicting_tmux_run_start(&req, Some(&the_builtin_coding_agents())).is_ok());
}

mod effort {
    use pretty_assertions::assert_eq;
    use tddy_rpc::Code;
    use tddy_service::proto::session::StartSessionRequest;

    use super::the_builtin_coding_agents;
    use crate::start_request_checks::refuse_an_effort_the_agent_cannot_take;

    fn a_start_of(session_type: &str, effort: &str) -> StartSessionRequest {
        StartSessionRequest {
            session_type: session_type.to_string(),
            effort: effort.to_string(),
            ..Default::default()
        }
    }

    fn the_answer_to(start: &StartSessionRequest) -> Result<(), (Code, String)> {
        refuse_an_effort_the_agent_cannot_take(start, Some(&the_builtin_coding_agents()))
            .map_err(|status| (status.code, status.message))
    }

    #[test]
    fn an_empty_effort_is_admitted_for_an_agent_with_levels_and_one_without() {
        // When
        let answers = (
            the_answer_to(&a_start_of("claude-cli", "")),
            the_answer_to(&a_start_of("opencode-cli", "")),
        );

        // Then — guard: the agent's own default needs no levels
        assert_eq!(answers, (Ok(()), Ok(())));
    }

    #[test]
    fn a_blank_effort_reads_as_the_agents_default() {
        // When
        let answer = the_answer_to(&a_start_of("opencode-cli", "  "));

        // Then — guard
        assert_eq!(answer, Ok(()));
    }

    #[test]
    fn a_level_the_agent_declares_is_admitted() {
        // When
        let answer = the_answer_to(&a_start_of("claude-cli", "high"));

        // Then — guard: admitted today because nothing is checked
        assert_eq!(answer, Ok(()));
    }

    #[test]
    fn a_level_differing_only_in_case_is_refused() {
        // When
        let answer = the_answer_to(&a_start_of("claude-cli", "High"));

        // Then — the levels are the agent's own words, matched exactly
        assert_eq!(
            answer,
            Err((
                Code::InvalidArgument,
                "effort \"High\" is not one of claude's effort levels (low, medium, high, xhigh, max)"
                    .to_string()
            ))
        );
    }

    #[test]
    fn an_effort_for_a_session_type_no_agent_declares_is_refused_naming_it() {
        // When
        let answer = the_answer_to(&a_start_of("workspace", "high"));

        // Then — a workspace session runs no agent to carry it
        assert_eq!(
            answer,
            Err((
                Code::InvalidArgument,
                "effort is not supported for workspace sessions: they run no coding agent"
                    .to_string()
            ))
        );
    }
}
