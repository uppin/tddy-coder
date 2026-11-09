//! The generic launch's claude/cursor steps (`#agent-tools 14/19`), each against the built-in spec
//! that declares the slot or capability it reads.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, RunSection};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::{builtin_specs, AgentEnvBase, CodingAgentSection, CodingAgentSpec};
use tddy_core::adapters::ConversationMintError;
use tddy_core::{HookCommandParams, SessionMetadata};
use tddy_rpc::Code;

use super::*;
use crate::cli_agent_steps::conversation::conversation_mint_refused;
use crate::cli_agent_steps::conversation::refuse_a_role_the_agent_cannot_hold;
use crate::cli_agent_steps::conversation::resumed_conversation;
use crate::cli_agent_steps::conversation::start_conversation;
use crate::cli_agent_steps::conversation::ConversationStart;
use crate::cli_agent_steps::conversation::LaunchConversation;
use crate::harness_from_spec::HarnessLaunch;
use crate::spec_launch::HostAgent;

/// The built-in `id`'s embedded YAML.
fn the_builtin_yaml(id: &str) -> &'static str {
    let file_name = format!("/{id}.yaml");
    builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .map(|(_, yaml)| yaml)
        .unwrap_or_else(|| panic!("{id}.yaml is built in"))
}

/// The coding-agent view of `yaml`.
fn the_spec_in(id: &str, yaml: &str) -> CodingAgentSpec {
    let file: serde_yaml::Value = serde_yaml::from_str(yaml).expect("the spec is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["extensions"]["coding_agent"].clone())
            .expect("its coding_agent section");
    CodingAgentSpec {
        id: id.to_string(),
        run,
        agent,
    }
}

fn the_builtin(id: &str) -> CodingAgentSpec {
    the_spec_in(id, the_builtin_yaml(id))
}

/// `cursor.yaml` whose mint is bounded at `secs`, replacing any conversation line it declares.
fn cursor_minting_within(secs: u64) -> CodingAgentSpec {
    let yaml: String = the_builtin_yaml("cursor")
        .lines()
        .filter(|line| !line.trim_start().starts_with("conversation:"))
        .map(|line| format!("{line}\n"))
        .collect();
    let section = "  coding_agent:\n";
    let bounded = yaml.replacen(
        section,
        &format!(
            "{section}    conversation: {{ kind: minted, adapter: cursor-create-chat, mint_deadline_secs: {secs} }}\n"
        ),
        1,
    );
    the_spec_in("cursor", &bounded)
}

/// A stub `agent` in `dir` whose `create-chat` runs `body`.
fn a_stub_agent(dir: &Path, body: &str) -> PathBuf {
    let binary = dir.join("agent");
    std::fs::write(&binary, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

fn a_recorded_session(session_id: &str) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        ..tddy_session_split::service_util::starting_session_metadata(
            session_id,
            "a-project",
            "acme-cli",
        )
    }
}

#[tokio::test]
async fn a_daemon_session_id_conversation_starts_as_the_session_id() {
    // Given
    let worktree = tempfile::tempdir().unwrap();

    // When
    let conversation = start_conversation(ConversationStart {
        spec: &the_builtin("claude"),
        binary: Path::new("/usr/local/bin/claude"),
        env_base: &AgentEnvBase::builtin(),
        worktree: worktree.path(),
        env: &[],
        session_id: "0190-claude",
    })
    .await
    .map_err(|status| status.message);

    // Then
    assert_eq!(
        conversation,
        Ok(LaunchConversation::SessionId("0190-claude".to_string()))
    );
}

#[tokio::test]
async fn a_minted_conversation_starts_as_the_id_its_adapter_mints() {
    // Given — a stub whose `create-chat` prints a bare chat id
    let bin = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(bin.path(), "echo chat-0042");

    // When
    let conversation = start_conversation(ConversationStart {
        spec: &the_builtin("cursor"),
        binary: &agent,
        env_base: &AgentEnvBase::builtin(),
        worktree: bin.path(),
        env: &[],
        session_id: "0190-cursor",
    })
    .await
    .map_err(|status| status.message);

    // Then
    assert_eq!(
        conversation,
        Ok(LaunchConversation::Minted("chat-0042".to_string()))
    );
}

#[tokio::test]
async fn a_mint_starts_from_the_base_set_of_the_daemons_environment_plus_its_launch_env() {
    // Given — a `create-chat` that names, in its chat id, whether it sees `HOME` (a base-set
    // variable this process holds) and the variable its launch set
    let bin = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(
        bin.path(),
        r#"printf 'chat-%s-%s\n' "${HOME:+home}" "${GIT_AUTHOR_NAME:-none}""#,
    );

    // When
    let conversation = start_conversation(ConversationStart {
        spec: &the_builtin("cursor"),
        binary: &agent,
        env_base: &AgentEnvBase::builtin(),
        worktree: bin.path(),
        env: &[("GIT_AUTHOR_NAME".to_string(), "dev".to_string())],
        session_id: "0190-cursor",
    })
    .await
    .map_err(|status| status.message);

    // Then
    assert_eq!(
        conversation,
        Ok(LaunchConversation::Minted("chat-home-dev".to_string()))
    );
}

#[tokio::test]
async fn a_worktree_conversation_starts_with_nothing_to_bind() {
    // Given
    let worktree = tempfile::tempdir().unwrap();

    // When
    let conversation = start_conversation(ConversationStart {
        spec: &the_builtin("opencode"),
        binary: Path::new("/usr/local/bin/opencode"),
        env_base: &AgentEnvBase::builtin(),
        worktree: worktree.path(),
        env: &[],
        session_id: "0190-opencode",
    })
    .await
    .map_err(|status| status.message);

    // Then
    assert_eq!(conversation, Ok(LaunchConversation::Worktree));
}

#[tokio::test]
async fn a_mint_past_its_deadline_refuses_the_start_naming_the_agent_the_adapter_and_the_deadline()
{
    // Given — a mint bounded at one second, and a `create-chat` that never answers
    let bin = tempfile::tempdir().unwrap();
    let agent = a_stub_agent(bin.path(), "exec /bin/sleep 3600");

    // When
    let refused = start_conversation(ConversationStart {
        spec: &cursor_minting_within(1),
        binary: &agent,
        env_base: &AgentEnvBase::builtin(),
        worktree: bin.path(),
        env: &[],
        session_id: "0190-cursor",
    })
    .await
    .map_err(|status| (status.code, status.message));

    // Then
    assert_eq!(
        refused,
        Err((
            Code::DeadlineExceeded,
            "cursor could not mint its conversation with cursor-create-chat: no conversation id \
             within 1s; the mint was killed"
                .to_string()
        ))
    );
}

#[test]
fn a_resume_binds_the_conversation_its_session_recorded() {
    // Given
    let claude = a_recorded_session("0190-claude");
    let cursor = SessionMetadata {
        cursor_chat_id: Some("chat-0042".to_string()),
        ..a_recorded_session("0190-cursor")
    };
    let warp = SessionMetadata {
        harness_resume_token: Some("tok-123".to_string()),
        ..a_recorded_session("0190-warp")
    };

    // When
    let resumed = (
        resumed_conversation(&the_builtin("claude"), &claude).map_err(|s| s.message),
        resumed_conversation(&the_builtin("cursor"), &cursor).map_err(|s| s.message),
        resumed_conversation(&the_builtin("warp"), &warp).map_err(|s| s.message),
        resumed_conversation(&the_builtin("opencode"), &claude).map_err(|s| s.message),
    );

    // Then
    assert_eq!(
        resumed,
        (
            Ok(LaunchConversation::SessionId("0190-claude".to_string())),
            Ok(LaunchConversation::Minted("chat-0042".to_string())),
            Ok(LaunchConversation::ResumeToken(Some("tok-123".to_string()))),
            Ok(LaunchConversation::Worktree),
        )
    );
}

#[test]
fn a_conversation_binds_its_own_template_parameter_and_records_only_a_minted_id() {
    // Given
    let session = LaunchConversation::SessionId("0190-claude".to_string());
    let minted = LaunchConversation::Minted("chat-0042".to_string());
    let token = LaunchConversation::ResumeToken(Some("tok-123".to_string()));

    // When
    let bound = (
        session.bound(HarnessLaunch::default()),
        minted.bound(HarnessLaunch::default()),
        token.bound(HarnessLaunch::default()),
        LaunchConversation::Worktree.bound(HarnessLaunch::default()),
    );
    let recorded = (session.recorded_chat_id(), minted.recorded_chat_id());

    // Then
    assert_eq!(
        (bound, recorded),
        (
            (
                HarnessLaunch {
                    session_id: Some("0190-claude"),
                    ..HarnessLaunch::default()
                },
                HarnessLaunch {
                    chat_id: Some("chat-0042"),
                    ..HarnessLaunch::default()
                },
                HarnessLaunch {
                    resume_token: Some("tok-123"),
                    ..HarnessLaunch::default()
                },
                HarnessLaunch::default(),
            ),
            (None, Some("chat-0042")),
        )
    );
}

#[test]
fn a_mint_that_failed_rather_than_timed_out_is_refused_failed_precondition() {
    // When
    let refused = conversation_mint_refused(
        "acme",
        "cursor-create-chat",
        &ConversationMintError::Failed("`agent create-chat` exited with 1: not logged in".into()),
    );

    // Then
    assert_eq!(
        (refused.code, refused.message),
        (
            Code::FailedPrecondition,
            "acme could not mint its conversation with cursor-create-chat: `agent create-chat` \
             exited with 1: not logged in"
                .to_string()
        )
    );
}

#[test]
fn a_project_home_main_is_held_only_by_an_agent_declaring_project_home_main() {
    // When
    let held = (
        refuse_a_role_the_agent_cannot_hold(
            &the_builtin("claude"),
            tddy_core::session_metadata::PROJECT_HOME_ROLE,
        )
        .map_err(|s| (s.code, s.message)),
        refuse_a_role_the_agent_cannot_hold(
            &the_builtin("opencode"),
            tddy_core::session_metadata::PROJECT_HOME_ROLE,
        )
        .map_err(|s| (s.code, s.message)),
        refuse_a_role_the_agent_cannot_hold(&the_builtin("opencode"), "")
            .map_err(|s| (s.code, s.message)),
    );

    // Then
    assert_eq!(
        held,
        (
            Ok(()),
            Err((
                Code::FailedPrecondition,
                "opencode cannot run a project's main: its spec declares no project_home_main"
                    .to_string()
            )),
            Ok(()),
        )
    );
}

fn resolved(id: &str) -> ResolvedTool {
    ResolvedTool {
        id: id.to_string(),
        command: id.to_string(),
        binary: PathBuf::from(format!("/usr/local/bin/{id}")),
        reads: Vec::new(),
        network: Default::default(),
    }
}

#[test]
fn a_host_launch_of_an_agent_declaring_hooks_renders_its_hooks_file_and_appended_prompt() {
    // Given — claude.yaml, a session's hooks and a recipe prompt file
    let worktree = tempfile::tempdir().unwrap();
    let session_dir = tempfile::tempdir().unwrap();
    let prompt = session_dir.path().join("recipe-prompt.md");
    let agent = HostAgent {
        spec: the_builtin("claude"),
        tool: resolved("claude"),
        deps: Vec::new(),
        env_base: tddy_coding_agent_specs::AgentEnvBase::builtin(),
    };
    let hooks = HookCommandParams {
        tddy_tools_path: "/usr/local/bin/tddy-tools",
        daemon_url: "http://127.0.0.1:8899",
        session_id: "0190-claude",
        os_user: "dev",
        hook_token: "a-hook-token",
        daemon_socket: None,
        answer_wait_secs: 120,
    };

    // When
    let rendered = render_host_launch_slots(
        &agent,
        worktree.path(),
        session_dir.path(),
        HostLaunchSlots {
            hooks: Some(hooks),
            system_prompt_file: Some(&prompt),
            ..HostLaunchSlots::default()
        },
    )
    .map(|slots| {
        (
            slots.argv_extra,
            slots
                .files
                .into_iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>(),
        )
    })
    .map_err(|s| s.message);

    // Then
    assert_eq!(
        rendered,
        Ok((
            vec![
                "--append-system-prompt-file".to_string(),
                prompt.to_string_lossy().into_owned(),
            ],
            vec![worktree.path().join(".claude/settings.local.json")],
        ))
    );
}

#[test]
fn a_local_checkouts_slot_files_are_left_for_the_local_write() {
    // Given
    let mut slots = RenderedSlots {
        files: vec![(
            PathBuf::from("/w/.claude/settings.local.json"),
            b"{}".to_vec(),
        )],
        ..RenderedSlots::default()
    };
    let before = slots.clone();

    // When
    let written = write_slot_files_on_target(&mut slots, None).map_err(|s| s.message);

    // Then
    assert_eq!((written, slots), (Ok(()), before));
}
