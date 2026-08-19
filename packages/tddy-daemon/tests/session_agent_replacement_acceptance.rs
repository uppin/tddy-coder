//! Acceptance tests: what the main agent loses when agents are attached, and what it gets back
//! when they are detached.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC19, AC21-AC25)
//!
//! `replaces` used to carry per-tool meaning: replacing `Shell` made a def the session's "action
//! author" and handed it `request_action`/`invoke_action`; replacing `Write` made it the "coder";
//! at most one def could replace `Shell`; and a def had to bind the tool it replaced. All of that
//! is gone. What is left is one rule — the union of every attached agent's `replaces` is withdrawn
//! from the main agent — and one mechanism that makes the rule real: a withdrawn tool's *native*
//! Claude aliases are hard-disabled too, or withdrawing `Shell` while leaving `Bash` withdraws
//! nothing.
//!
//! AC20 (the runtime refusal on a live roster) is pinned in `tddy-tools`, where the call is
//! actually made; this suite pins what the spawn allowlist is built from.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::connection_service::{roster_replacement_pairs, ConnectionServiceImpl};
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_discovery::subagent::normalize_replaced_tools;
use tddy_rpc::{Code, Request};
use tddy_sandbox_recipes::{build_claude_allowlist, build_claude_disallowlist};
use tddy_service::proto::connection::{
    AttachSessionAgentRequest, ConnectionService as ConnectionServiceTrait,
    DetachSessionAgentRequest, ListSubagentsRequest, SessionAgentRoster,
};

/// The session-action tools a `Shell`-replacing def used to be granted automatically.
const SESSION_ACTION_TOOLS: &[&str] = &[
    "mcp__tddy-tools__request_action",
    "mcp__tddy-tools__list_actions",
    "mcp__tddy-tools__invoke_action",
];

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

struct RosteredSession {
    service: ConnectionServiceImpl,
    session_id: String,
    sessions: tempfile::TempDir,
}

impl RosteredSession {
    async fn attach(&self, agent_id: &str) -> Result<SessionAgentRoster, tddy_rpc::Status> {
        self.service
            .attach_session_agent(Request::new(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn detach(&self, agent_id: &str) -> SessionAgentRoster {
        self.service
            .detach_session_agent(Request::new(DetachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .expect("detach must succeed")
            .into_inner()
    }

    async fn agent_id_for(&self, name: &str) -> String {
        self.service
            .list_subagents(Request::new(ListSubagentsRequest {}))
            .await
            .expect("listing subagents must succeed")
            .into_inner()
            .subagents
            .into_iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("this daemon must advertise a def named '{name}'"))
            .agent_id
    }

    fn session_dir(&self) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), &self.session_id)
    }

    /// The roster as `.session.yaml` holds it — the exact value a spawn is handed
    /// (`resume_sandboxed_session` reads `read_session_metadata(...).agents` and passes it to
    /// `relaunch_sandboxed_runner`), so this is the roster the daemon would launch from, not a
    /// snapshot cached in the service that attached the agent.
    fn persisted_roster(&self) -> Vec<tddy_core::SessionAgentRecord> {
        tddy_core::read_session_metadata(&self.session_dir())
            .expect("the session's metadata must be readable")
            .agents
    }

    /// The tools the *persisted* roster withdraws from the main agent, as every spawn path computes
    /// them: `roster_replacement_pairs` over that roster, unioned across its agents.
    fn withdrawn_tools(&self) -> Vec<String> {
        withdrawn_by(&self.persisted_roster())
    }
}

/// What `agents` costs the main agent: the production per-agent pairs, unioned.
///
/// The union is the rule (AC19), and it is spelled with the same normalizer the pairs are built
/// with, so two agents replacing one tool withdraw it once.
fn withdrawn_by(agents: &[tddy_core::SessionAgentRecord]) -> Vec<String> {
    normalize_replaced_tools(
        &roster_replacement_pairs(agents)
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect::<Vec<String>>(),
    )
}

fn a_managed_session_with_agents_available(agents: &[(&str, &str)]) -> RosteredSession {
    a_session_of_type_with_agents_available("claude-cli", true, agents)
}

/// A session of `session_type`, `managed` or not, on a daemon whose agents directory holds `agents`
/// as `(name, replaces-csv)` pairs.
fn a_session_of_type_with_agents_available(
    session_type: &str,
    managed: bool,
    agents: &[(&str, &str)],
) -> RosteredSession {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_agent_defs(sessions.path(), agents);

    let session_id = "1780828020298-replacement".to_string();
    let session_dir = unified_session_dir_path(sessions.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(&session_dir, &a_session(&session_id, session_type, managed))
        .expect("write session metadata");

    RosteredSession {
        service: test_service(sessions.path().to_path_buf()),
        session_id,
        sessions,
    }
}

fn write_agent_defs(tddy_data_dir: &Path, agents: &[(&str, &str)]) {
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    for (name, replaces) in agents {
        std::fs::write(
            agents_dir.join(format!("{name}.yaml")),
            format!(
                "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n\
                 tools: [READ, GLOB, GREP]\nreplaces: [{replaces}]\n"
            ),
        )
        .expect("write agent def");
    }
}

fn a_session(session_id: &str, session_type: &str, managed: bool) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "project-under-replacement".to_string(),
        created_at: "2026-08-16T10:00:00Z".to_string(),
        updated_at: "2026-08-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/replacement".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some(session_type.to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: Some(managed),
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait ToolListAssertions {
    fn assert_contains(&self, tool: &str) -> &Self;
    fn assert_omits(&self, tool: &str) -> &Self;
}

impl ToolListAssertions for Vec<String> {
    fn assert_contains(&self, tool: &str) -> &Self {
        assert!(
            self.iter().any(|t| t == tool),
            "expected '{tool}' in {self:?}"
        );
        self
    }

    fn assert_omits(&self, tool: &str) -> &Self {
        assert!(
            !self.iter().any(|t| t == tool),
            "expected '{tool}' to be absent from {self:?}"
        );
        self
    }
}

// ---------------------------------------------------------------------------
// AC19 / AC21 — the union, and getting it back
// ---------------------------------------------------------------------------

/// Two agents, one replaced tool each, both withdrawn. The union is the rule; there is no notion of
/// a primary agent whose list wins.
#[tokio::test]
async fn withdraws_every_tool_any_attached_agent_replaces() {
    // Given
    let session =
        a_managed_session_with_agents_available(&[("explorer", "Grep"), ("linter", "ReadLints")]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;

    // When
    session.attach(&explorer).await.expect("attach explorer");
    session.attach(&linter).await.expect("attach linter");

    // Then
    let withdrawn = session.withdrawn_tools();
    assert_eq!(
        withdrawn,
        vec!["Grep".to_string(), "ReadLints".to_string()],
        "the withdrawn set is the union of every attached agent's replaces"
    );
}

/// Two agents replacing the same tool withdraw it once — and detaching one of them does **not**
/// give it back, because the other is still serving it.
#[tokio::test]
async fn keeps_a_tool_withdrawn_while_any_agent_still_replaces_it() {
    // Given
    let session =
        a_managed_session_with_agents_available(&[("explorer", "Grep"), ("searcher", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;
    let searcher = session.agent_id_for("searcher").await;
    session.attach(&explorer).await.expect("attach explorer");
    session.attach(&searcher).await.expect("attach searcher");

    // When
    session.detach(&explorer).await;

    // Then
    session.withdrawn_tools().assert_contains("Grep");
}

/// Detaching the last agent replacing a tool restores it. Without this, a session that briefly
/// attached a search agent would be permanently unable to grep.
#[tokio::test]
async fn restores_a_tool_once_the_last_agent_replacing_it_is_detached() {
    // Given
    let session = a_managed_session_with_agents_available(&[("explorer", "Grep, Glob")]);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("attach explorer");
    session.withdrawn_tools().assert_contains("Grep");

    // When
    session.detach(&explorer).await;

    // Then
    let withdrawn = session.withdrawn_tools();
    assert_eq!(
        withdrawn,
        Vec::<String>::new(),
        "an empty roster must withdraw nothing"
    );
}

// ---------------------------------------------------------------------------
// AC22-AC23 — no per-tool meaning
// ---------------------------------------------------------------------------

/// The at-most-one-Shell-replacer rule is gone. With qualified ids the main agent names which of
/// the two it wants, so the ambiguity the rule existed to prevent no longer exists.
#[tokio::test]
async fn accepts_two_agents_that_both_replace_the_shell_tool() {
    // Given
    let session =
        a_managed_session_with_agents_available(&[("runner-a", "Shell"), ("runner-b", "Shell")]);
    let runner_a = session.agent_id_for("runner-a").await;
    let runner_b = session.agent_id_for("runner-b").await;

    // When
    session.attach(&runner_a).await.expect("attach runner-a");
    let roster = session
        .attach(&runner_b)
        .await
        .expect("a second Shell-replacing agent must be accepted");

    // Then
    assert_eq!(roster.agents.len(), 2);
    assert_eq!(session.withdrawn_tools(), vec!["Shell".to_string()]);
}

/// Replacing `Shell` no longer confers the session-action surface. That grant was a policy
/// (no-bash mode) encoded into the mechanism that binds agents to tools.
#[test]
fn replacing_shell_no_longer_grants_the_session_action_tools() {
    // Given
    let replaced = ["Shell"];

    // When
    let allowlist = build_claude_allowlist(true, &replaced);

    // Then
    allowlist
        .assert_omits(SESSION_ACTION_TOOLS[0])
        .assert_omits(SESSION_ACTION_TOOLS[1])
        .assert_omits(SESSION_ACTION_TOOLS[2]);
}

/// The native `Bash` family stays hard-disabled when `Shell` is replaced. This is not the removed
/// role — it is what makes the withdrawal real, since leaving `Bash` callable withdraws nothing.
#[test]
fn keeps_the_native_bash_family_unreachable_when_shell_is_replaced() {
    // Given
    let replaced = ["Shell"];

    // When
    let disallowed = build_claude_disallowlist(&replaced);

    // Then
    disallowed
        .assert_contains("Shell")
        .assert_contains("mcp__tddy-tools__Shell")
        .assert_contains("Bash")
        .assert_contains("BashOutput")
        .assert_contains("KillShell");
}

/// Same property for the write family: replacing `Write` must also close `Edit`/`MultiEdit`/
/// `NotebookEdit`, or the agent simply edits by another name.
#[test]
fn keeps_the_native_edit_family_unreachable_when_write_is_replaced() {
    // Given
    let replaced = ["Write"];

    // When
    let disallowed = build_claude_disallowlist(&replaced);

    // Then
    disallowed
        .assert_contains("Write")
        .assert_contains("Edit")
        .assert_contains("MultiEdit")
        .assert_contains("NotebookEdit");
}

/// An agent may declare it replaces a tool its own loop does not bind. The def that motivated the
/// field did exactly this — replacing `SemanticSearch` and answering it from a read/glob/grep loop
/// — so the correspondence check would have outlawed its own reason for existing.
#[tokio::test]
async fn accepts_an_agent_that_replaces_a_tool_it_cannot_serve_itself() {
    // Given — bound tools are READ/GLOB/GREP; the def replaces SemanticSearch, which it has no
    // matching binding for.
    let session = a_managed_session_with_agents_available(&[("explorer", "SemanticSearch")]);
    let explorer = session.agent_id_for("explorer").await;

    // When
    let roster = session
        .attach(&explorer)
        .await
        .expect("an agent may replace a tool it does not bind");

    // Then
    assert_eq!(roster.agents.len(), 1);
    assert_eq!(
        session.withdrawn_tools(),
        vec!["SemanticSearch".to_string()]
    );
}

// ---------------------------------------------------------------------------
// AC24 — where a withdrawal cannot be enforced, it is not offered
// ---------------------------------------------------------------------------

/// A non-managed session's main agent has native filesystem tools that never pass through
/// `tddy-tools`, so a live withdrawal there could only be advisory. Rather than accept the attach
/// and quietly not enforce it, the attach is refused.
#[tokio::test]
async fn refuses_a_replacing_agent_on_a_session_whose_tools_it_cannot_reach() {
    // Given
    let session =
        a_session_of_type_with_agents_available("claude-cli", false, &[("explorer", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;

    // When
    let result = session.attach(&explorer).await;

    // Then
    let status = result.expect_err("a replacing agent must be refused on a non-managed session");
    assert_eq!(status.code(), Code::FailedPrecondition);
    assert!(
        status.message().contains("Grep"),
        "the refusal must name the tool it could not withdraw, was: {}",
        status.message()
    );
}

/// An agent that replaces nothing has nothing to enforce, so it attaches to a non-managed session
/// perfectly well — the refusal above is about withdrawal, not about remote agents in general.
#[tokio::test]
async fn accepts_a_non_replacing_agent_on_a_non_managed_session() {
    // Given
    let session = a_session_of_type_with_agents_available("claude-cli", false, &[("reviewer", "")]);
    let reviewer = session.agent_id_for("reviewer").await;

    // When
    let roster = session
        .attach(&reviewer)
        .await
        .expect("an agent that withdraws nothing must attach anywhere");

    // Then
    assert_eq!(roster.agents.len(), 1);
}

// ---------------------------------------------------------------------------
// AC25 — the spawn path reads the roster
// ---------------------------------------------------------------------------

/// A tool withdrawn by an agent attached mid-session must still be withdrawn after a relaunch. The
/// allowlist is built from the persisted roster, not from the names the original start request
/// carried — those did not include the agent attached at minute forty.
#[tokio::test]
async fn launches_a_resumed_session_without_the_tools_its_roster_replaced() {
    // Given — an agent attached after the session started
    let session = a_managed_session_with_agents_available(&[("explorer", "Grep, Glob")]);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("attach explorer");

    // When — the roster is read back off disk and run through the spawn path's own
    // `roster_replacement_pairs`, which is what a relaunch does with it: nothing of the attach
    // survives in memory here, so a roster the attach failed to persist withdraws nothing.
    let withdrawn = withdrawn_by(&session.persisted_roster());
    let withdrawn_refs: Vec<&str> = withdrawn.iter().map(String::as_str).collect();
    let allowlist = build_claude_allowlist(true, &withdrawn_refs);
    let disallowed = build_claude_disallowlist(&withdrawn_refs);

    // Then
    assert_eq!(withdrawn, vec!["Grep".to_string(), "Glob".to_string()]);
    allowlist
        .assert_omits("mcp__tddy-tools__Grep")
        .assert_omits("mcp__tddy-tools__Glob")
        .assert_contains("mcp__tddy-tools__Read");
    disallowed.assert_contains("Grep").assert_contains("Glob");
}

/// A session with an empty roster launches with its whole tool set — the ordinary case, and the
/// one that must not regress while the roster machinery is added around it.
#[tokio::test]
async fn launches_a_session_with_no_agents_holding_every_tool() {
    // Given
    let session = a_managed_session_with_agents_available(&[("explorer", "Grep")]);

    // When — nothing attached
    let withdrawn = session.withdrawn_tools();
    let withdrawn_refs: Vec<&str> = withdrawn.iter().map(String::as_str).collect();
    let allowlist = build_claude_allowlist(true, &withdrawn_refs);

    // Then
    assert_eq!(withdrawn, Vec::<String>::new());
    allowlist
        .assert_contains("mcp__tddy-tools__Grep")
        .assert_contains("mcp__tddy-tools__Glob")
        .assert_contains("mcp__tddy-tools__Shell");
}
