//! Acceptance: an agent is added by dropping one YAML file into `<tddyhome>/tools/`, and an agent's
//! gates are its declared capabilities rather than its name (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
//!
//! The daemon is the real `DaemonSessionHost` given the coding-agent registry over its
//! `<tddyhome>/tools` (see `agent_registry_support`); its daemon.yaml has no agent section. The
//! new agent, `acme`, names only adapters that already exist. The routed starts (`#agent-tools
//! 13/19`) run on a Meta account in the owner's unlocked vault, and the stub reports the route
//! environment it was given. The starts of `#agent-tools 14/19` drive the generic launch's managed
//! workflow, hooks, conversation slot and `TDDY_*`-free environment with dropped-in agents that
//! declare them. The jailed start runs a real Seatbelt jail with the real
//! `tddy-sandbox-runner`, so it is macOS-only: CI (ubuntu) never runs the
//! `jailed` module.

mod agent_registry_support;
mod support;

use std::collections::BTreeSet;

use agent_registry_support::{
    a_daemon_with_tools, a_registry_daemon, acme_yaml_declaring, builtin_yaml_run_as_the_stub,
    ACME_MODEL, ACME_YAML, META_ACCOUNT, META_ANTHROPIC_ROOT, META_KEY, MINTED_CHAT_ID,
};
use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{builtin_specs, Capability};
use tddy_rpc::Code;

/// The built-in claude.yaml with `hooks` struck from its capabilities — an operator's override.
fn claude_yaml_without_hooks() -> String {
    let (_, claude) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("/claude.yaml"))
        .expect("claude.yaml is built in");
    let without = claude.replace("capabilities: [hooks, ", "capabilities: [");
    assert_ne!(
        without, claude,
        "the override strikes hooks from claude.yaml"
    );
    without
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_dropped_into_tddyhome_tools_appears_in_the_hosts_agent_list_starts_on_the_host_and_offers_its_declared_capabilities(
) {
    // Given — acme.yaml, and nothing else about acme anywhere
    let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

    // When
    let listed: Vec<String> = daemon
        .registry
        .all()
        .iter()
        .map(|spec| spec.id.clone())
        .filter(|id| id == "acme" || id == "claude")
        .collect();
    let declared: BTreeSet<Capability> = daemon
        .registry
        .for_session_type("acme-cli")
        .expect("acme-cli names acme")
        .capabilities()
        .clone();
    let (session_id, _host) = daemon
        .start(daemon.a_start("acme-cli", false))
        .await
        .expect("a host acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.an_active_session("acme-hooked-1", "acme-cli");
    let hook_report = daemon.a_hook_reports_running("acme-hooked-1").await;

    // Then — listed beside the built-ins, started with acme.yaml's argv, and its declared `hooks`
    // admits the hook report a name gate would have refused
    assert_eq!(
        (
            listed,
            declared,
            started.argv(),
            daemon.metadata_of(&session_id).session_type,
            hook_report.map_err(|status| status.message),
            daemon.metadata_of("acme-hooked-1").activity_status,
        ),
        (
            vec!["acme".to_string(), "claude".to_string()],
            [Capability::Hooks, Capability::RequiresModel]
                .into_iter()
                .collect(),
            vec![
                daemon.stub_path(),
                "--model".to_string(),
                ACME_MODEL.to_string(),
            ],
            Some("acme-cli".to_string()),
            Ok(()),
            Some("Running".to_string()),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn removing_hooks_from_an_override_of_claude_yaml_turns_off_hook_based_activity_for_claude() {
    // Given — claude.yaml overridden without `hooks`; cursor.yaml as built in
    let claude = claude_yaml_without_hooks();
    let daemon = a_daemon_with_tools(&[("claude.yaml", claude.as_str())]);
    daemon.an_active_session("claude-unhooked-1", "claude-cli");
    daemon.an_active_session("cursor-hooked-1", "cursor-cli");

    // When
    let claude_report = daemon.a_hook_reports_running("claude-unhooked-1").await;
    let cursor_report = daemon.a_hook_reports_running("cursor-hooked-1").await;

    // Then — the claude-cli report is refused and records nothing; the cursor-cli one, whose spec
    // still declares `hooks`, is the positive control
    assert_eq!(
        (
            claude_report.map_err(|status| status.code),
            daemon.metadata_of("claude-unhooked-1").activity_status,
            cursor_report.map_err(|status| status.code),
            daemon.metadata_of("cursor-hooked-1").activity_status,
        ),
        (
            Err(Code::FailedPrecondition),
            None,
            Ok(()),
            Some("Running".to_string()),
        )
    );
}

/// The built-in claude.yaml run as the stub, with `branch_name_required` struck from its
/// capabilities — an operator's override.
fn claude_yaml_without_branch_name_required() -> String {
    let claude = builtin_yaml_run_as_the_stub("claude");
    let without = claude.replace("branch_name_required, ", "");
    assert_ne!(
        without, claude,
        "the override strikes branch_name_required from claude.yaml"
    );
    without
}

/// The branch `worktree` has checked out.
fn the_branch_checked_out_in(worktree: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree)
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Branch intent is the spec's (`#agent-tools 17/19`): a claude-cli start reads
/// `branch_name_required` from the claude spec it launches, not from a policy the claude launch path
/// hard-codes, so an override that drops the capability lets a blank new branch name through and the
/// branch is generated under the session type.
#[tokio::test]
#[serial_test::serial]
async fn removing_branch_name_required_from_an_override_of_claude_yaml_lets_a_blank_new_branch_name_generate_one(
) {
    // Given — claude.yaml overridden without branch_name_required
    let claude = claude_yaml_without_branch_name_required();
    let daemon = a_daemon_with_tools(&[("claude.yaml", claude.as_str())]);

    // When — a new branch from the base that names no branch
    let started = daemon
        .start(daemon.a_start("claude-cli", false))
        .await
        .map(|(session_id, _host)| session_id);

    // Then — the start succeeds on the branch generated for it
    let session_id = started
        .map_err(|status| (status.code, status.message))
        .expect("a claude-cli start with a blank new branch name succeeds");
    assert_eq!(
        the_branch_checked_out_in(&daemon.worktree_of(&session_id)),
        format!("claude-cli/{}", &session_id[..8])
    );
}

/// `acme.yaml` declaring `product: Acme Code` and a route to Anthropic Messages through its
/// environment — the shape and provider Claude Code's spec declares, under no name Rust knows.
fn acme_yaml_with_an_anthropic_env_route() -> String {
    acme_yaml_declaring(
        "    product: Acme Code\n    route:\n      providers: [anthropic]\n      shape: env\n",
    )
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_dropped_into_tddyhome_tools_with_an_anthropic_env_route_starts_on_a_provider_account_with_the_accounts_endpoint_in_its_environment(
) {
    // Given — acme.yaml with an anthropic env route, and the owner's Meta account, which serves
    // Anthropic Messages
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &acme_yaml_with_an_anthropic_env_route())
        .with_a_meta_account()
        .build();

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start_on_account("acme-cli", META_ACCOUNT))
        .await
        .expect("a host acme-cli start on the Meta account succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the Anthropic-env body's environment: the account's root, its key and the model
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_type,
            started.env("ANTHROPIC_BASE_URL"),
            started.env("ANTHROPIC_AUTH_TOKEN"),
            started.env("ANTHROPIC_MODEL"),
        ),
        (
            Some("acme-cli".to_string()),
            META_ANTHROPIC_ROOT.to_string(),
            META_KEY.to_string(),
            ACME_MODEL.to_string(),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_without_a_route_section_is_refused_on_a_provider_account_naming_the_agent_and_spawns_nothing(
) {
    // Given — acme.yaml declaring its product and no route, and the owner's Meta account
    let daemon = a_registry_daemon()
        .with_tool(
            "acme.yaml",
            &acme_yaml_declaring("    product: Acme Code\n"),
        )
        .with_a_meta_account()
        .build();

    // When
    let refusal = daemon
        .start(daemon.a_start_on_account("acme-cli", META_ACCOUNT))
        .await
        .map(|(session_id, _host)| session_id)
        .expect_err("an agent that declares no provider is not routed");

    // Then — refused naming the spec's product, and nothing is spawned
    assert_eq!(
        (refusal.code, refusal.message, daemon.spawn_marker.exists()),
        (
            Code::InvalidArgument,
            "Acme Code cannot be pointed at another provider; it always runs on its own \
             vendor"
                .to_string(),
            false,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_harness_profile_naming_a_dropped_in_agent_starts_its_session_type() {
    // Given — a profile running `acme-cli` on the Meta account, and acme.yaml routable to it
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &acme_yaml_with_an_anthropic_env_route())
        .with_profile(
            "acme-on-meta",
            &format!(
                "name: acme-on-meta\nharness: acme-cli\nprovider_account: {META_ACCOUNT}\nmodel: {ACME_MODEL}\n"
            ),
        )
        .with_a_meta_account()
        .build();

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start_through_profile("acme-on-meta"))
        .await
        .expect("a start through the profile succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the profile's harness is the session type, run on the profile's account
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_type,
            started.env("ANTHROPIC_BASE_URL"),
            started.env("ANTHROPIC_AUTH_TOKEN"),
        ),
        (
            Some("acme-cli".to_string()),
            META_ANTHROPIC_ROOT.to_string(),
            META_KEY.to_string(),
        )
    );
}

// --- The generic launch serves claude's and cursor's needs (`#agent-tools 14/19`) -------------

/// `acme.yaml` whose argv splices its slots and whose instructions take an appended system prompt
/// file, declaring `hooks` and `managed_workflow` — the shape a managed workflow needs, under no name
/// Rust knows.
fn acme_yaml_with_hooks_and_managed_workflow() -> String {
    let managed = ACME_YAML
        .replace(
            "    - { if: prompt, then: [\"{prompt}\"] }\n",
            "    - { splice: slots }\n    - { if: prompt, then: [\"{prompt}\"] }\n",
        )
        .replace(
            "      files: [AGENTS.md]\n",
            "      files: [AGENTS.md]\n      append_system_prompt: { flag: --append-system-prompt-file }\n",
        )
        .replace(
            "capabilities: [hooks, requires_model]",
            "capabilities: [hooks, requires_model, managed_workflow]",
        );
    assert_eq!(
        managed.matches("managed_workflow").count()
            + managed.matches("{ splice: slots }").count()
            + managed.matches("--append-system-prompt-file").count(),
        3,
        "acme.yaml gains a slots splice, an appended prompt and managed_workflow"
    );
    managed
}

/// `acme.yaml` whose conversation is the daemon's session id: `--session-id` on a start, `--resume`
/// with the same id on a resume.
fn acme_yaml_conversing_by_the_daemon_session_id() -> String {
    let declared = acme_yaml_declaring("    conversation: { kind: daemon_session_id }\n").replace(
        "    - { if: prompt, then: [\"{prompt}\"] }\n",
        "    - { if: resume, then: [\"--resume\", \"{session_id}\"], else: [\"--session-id\", \"{session_id}\"] }\n    - { if: prompt, then: [\"{prompt}\"] }\n",
    );
    assert!(
        declared.contains("--session-id"),
        "acme.yaml's argv names the session id"
    );
    declared
}

/// `acme.yaml` whose conversation is a chat `cursor-create-chat` mints, resumed as `--resume CHAT`.
fn acme_yaml_conversing_by_a_minted_chat() -> String {
    let declared = acme_yaml_declaring(
        "    conversation: { kind: minted, adapter: cursor-create-chat }\n",
    )
    .replace(
        "    - { if: prompt, then: [\"{prompt}\"] }\n",
        "    - { if: chat_id, then: [\"--resume\", \"{chat_id}\"] }\n    - { if: prompt, then: [\"{prompt}\"] }\n",
    );
    assert!(
        declared.contains("{chat_id}"),
        "acme.yaml's argv names the chat"
    );
    declared
}

#[tokio::test]
#[serial_test::serial]
async fn a_dropped_in_agent_declaring_hooks_and_managed_workflow_starts_over_a_managed_codebase_with_its_hooks_file_written_and_the_recipe_prompt_appended(
) {
    // Given
    let daemon =
        a_daemon_with_tools(&[("acme.yaml", &acme_yaml_with_hooks_and_managed_workflow())]);

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_managed_start("acme-cli", "tdd"))
        .await
        .expect("a managed acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — managed on the tdd recipe, claude-settings-local's hooks file in the worktree, and the
    // recipe's orchestration prompt handed over as a file that exists
    let meta = daemon.metadata_of(&session_id);
    let prompt_files: Vec<bool> = started
        .values_of("--append-system-prompt-file")
        .iter()
        .map(|path| std::path::Path::new(path).is_file())
        .collect();
    assert_eq!(
        (
            meta.managed_codebase,
            meta.recipe,
            daemon
                .worktree_of(&session_id)
                .join(".claude/settings.local.json")
                .is_file(),
            prompt_files,
        ),
        (true, Some("tdd".to_string()), true, vec![true])
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_dropped_in_agent_without_managed_workflow_is_refused_a_managed_codebase_naming_the_agent(
) {
    // Given — acme.yaml as dropped in: no managed_workflow
    let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

    // When
    let refusal = daemon
        .start(daemon.a_managed_start("acme-cli", "tdd"))
        .await
        .map(|(session_id, _host)| session_id)
        .expect_err("an agent without managed_workflow is not started over a managed codebase");

    // Then — refused naming the agent and the capability it lacks; nothing spawned
    assert_eq!(
        (refusal.code, refusal.message, daemon.spawn_marker.exists()),
        (
            Code::InvalidArgument,
            "managed_codebase is not supported for acme-cli sessions: acme declares no \
             managed_workflow"
                .to_string(),
            false,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_dropped_in_agent_whose_conversation_is_the_daemon_session_id_resumes_with_the_same_id() {
    // Given
    let daemon = a_daemon_with_tools(&[(
        "acme.yaml",
        &acme_yaml_conversing_by_the_daemon_session_id(),
    )]);

    // When — started, stopped, then resumed
    let (session_id, _host) = daemon
        .start(daemon.a_start("acme-cli", false))
        .await
        .expect("a host acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.stop(&session_id).await;
    daemon
        .resume(&session_id)
        .await
        .expect("the stopped acme-cli session resumes");
    let resumed = daemon.stub_report(&session_id).await;

    // Then — the session id names the conversation on both launches; the resume keeps its model
    let stub = daemon.stub_path();
    assert_eq!(
        (started.argv(), resumed.argv()),
        (
            vec![
                stub.clone(),
                "--model".to_string(),
                ACME_MODEL.to_string(),
                "--session-id".to_string(),
                session_id.clone(),
            ],
            vec![
                stub,
                "--model".to_string(),
                ACME_MODEL.to_string(),
                "--resume".to_string(),
                session_id.clone(),
            ],
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_dropped_in_agent_whose_conversation_is_minted_by_cursor_create_chat_resumes_its_minted_chat(
) {
    // Given — the stub's `create-chat` prints MINTED_CHAT_ID
    let daemon = a_daemon_with_tools(&[("acme.yaml", &acme_yaml_conversing_by_a_minted_chat())]);

    // When — started, stopped, then resumed
    let (session_id, _host) = daemon
        .start(daemon.a_start("acme-cli", false))
        .await
        .expect("a host acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.stop(&session_id).await;
    daemon
        .resume(&session_id)
        .await
        .expect("the stopped acme-cli session resumes");
    let resumed = daemon.stub_report(&session_id).await;

    // Then — the minted chat is recorded and named by both launches
    assert_eq!(
        (
            started.values_of("--resume"),
            resumed.values_of("--resume"),
            daemon.metadata_of(&session_id).cursor_chat_id,
        ),
        (
            vec![MINTED_CHAT_ID.to_string()],
            vec![MINTED_CHAT_ID.to_string()],
            Some(MINTED_CHAT_ID.to_string()),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_agent_process_of_a_dropped_in_agent_sees_no_tddy_variable() {
    // Given — a TDDY_ variable in the daemon's own environment, and a dropped-in agent declaring
    // hooks, managed workflow and a daemon-id conversation
    let _leak = agent_registry_support::ATddyVariableInTheDaemon::set("TDDY_LEAK_PROBE", "1");
    let daemon =
        a_daemon_with_tools(&[("acme.yaml", &acme_yaml_with_hooks_and_managed_workflow())]);

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_managed_start("acme-cli", "tdd"))
        .await
        .expect("a managed acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then
    assert_eq!(started.tddy_variables(), Vec::<String>::new());
}

#[tokio::test]
#[serial_test::serial]
async fn the_agent_process_of_a_dropped_in_agent_started_in_tmux_sees_no_tddy_variable() {
    // Given — a TDDY_ variable in the daemon's own environment before its private tmux server
    // starts (a pane inherits the server's environment, not the daemon's), and the dropped-in agent
    // declaring hooks, managed workflow and a daemon-id conversation
    let _leak = agent_registry_support::ATddyVariableInTheDaemon::set("TDDY_LEAK_PROBE", "1");
    let _tmux = support::PrivateTmux::started();
    let daemon =
        a_daemon_with_tools(&[("acme.yaml", &acme_yaml_with_hooks_and_managed_workflow())]);

    // When — a managed start in tmux
    let start = tddy_service::proto::session::StartSessionRequest {
        run_in_tmux: true,
        ..daemon.a_managed_start("acme-cli", "tdd")
    };
    let (session_id, _host) = daemon
        .start(start)
        .await
        .expect("a managed acme-cli start in tmux succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the pane runs the agent with no TDDY_ variable at all
    assert_eq!(
        (
            daemon.metadata_of(&session_id).tmux_session.is_some(),
            started.tddy_variables(),
        ),
        (true, Vec::<String>::new())
    );
}

/// The jailed half of the YAML-only claim: macOS only, through a real Seatbelt jail.
#[cfg(target_os = "macos")]
mod jailed {
    use pretty_assertions::assert_eq;

    use super::agent_registry_support::{a_daemon_with_tools, ACME_MODEL, ACME_YAML};

    #[tokio::test]
    #[serial_test::serial]
    async fn a_new_agent_dropped_into_tddyhome_tools_starts_in_a_jail() {
        // Given
        let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

        // When
        let (session_id, _jail) = daemon
            .start(daemon.a_start("acme-cli", true))
            .await
            .expect("a jailed acme-cli start succeeds");
        let started = daemon.stub_report(&session_id).await;

        // Then — acme.yaml's argv, run inside the jail
        let meta = daemon.metadata_of(&session_id);
        assert_eq!(
            (
                started.argv().into_iter().skip(1).collect::<Vec<_>>(),
                meta.session_type,
                meta.sandbox,
            ),
            (
                vec!["--model".to_string(), ACME_MODEL.to_string()],
                Some("acme-cli".to_string()),
                Some(true),
            )
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn the_agent_process_of_a_dropped_in_agent_in_a_jail_sees_no_tddy_variable() {
        // Given — a TDDY_ variable in the daemon's own environment
        let _leak =
            super::agent_registry_support::ATddyVariableInTheDaemon::set("TDDY_LEAK_PROBE", "1");
        let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

        // When
        let (session_id, _jail) = daemon
            .start(daemon.a_start("acme-cli", true))
            .await
            .expect("a jailed acme-cli start succeeds");
        let started = daemon.stub_report(&session_id).await;

        // Then
        assert_eq!(started.tddy_variables(), Vec::<String>::new());
    }
}
