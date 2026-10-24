//! Acceptance: the daemon side of `subagent_diff`, `subagent_pull` and `subagent_end` for a daemon-run
//! clone conversation, on a real git session worktree.
//!
//! A clone-placed agent's conversation has a worktree of its own, and the three tools hand it back
//! through the daemon's `ConversationWorktree` operations: `Diff` shows what it committed, `PullRange`
//! brings that work into the session worktree, and `Remove` ends the conversation (worktree, branch and
//! base ref). A shared conversation has no worktree to hand back, so the same operations have nothing
//! to act on. The routing decision itself is `tddy-tools`' `has_its_own_worktree`, pinned in its
//! `worktree_route_acceptance`; these tests drive the operations that decision sends the tools to.
//! No `tddy-tools` process runs here.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-clone-placement.md

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, SessionMetadata};
use tddy_daemon_rpc::test_util::{test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::{Code, Request};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, DiffOp, ExecToolService as _,
    PullRangeOp, RemoveOp,
};
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, OpenAgentConversationRequest, PromptAgentConversationRequest,
    SessionAgentService as _,
};
use tddy_service::proto::types::AgentPlacement;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const SESSION_ID: &str = "1780828020298-routing";
const CODEX: &str = "codex-fast";
/// The file the coding agent writes on its turn, relative to its working directory.
const WRITTEN: &str = "notes/from-agent.txt";
const WRITTEN_CONTENTS: &str = "from the coding agent\n";
const TURN_DEADLINE: Duration = Duration::from_secs(10);

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// Records every launch, and starts the stub with a scenario whose one turn writes [`WRITTEN`].
struct WritingLauncher {
    dir: PathBuf,
    launches: Mutex<Vec<CodingAgentLaunch>>,
}

#[async_trait::async_trait]
impl CodingAgentLauncher for WritingLauncher {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        self.launches.lock().expect("launches").push(launch.clone());
        let scenario = self.dir.join("scenario.json");
        std::fs::write(
            &scenario,
            serde_json::json!({
                "responses": [{
                    "chunks": ["noted"],
                    "write_files": [{ "path": WRITTEN, "content": WRITTEN_CONTENTS }],
                }]
            })
            .to_string(),
        )
        .expect("write scenario");
        Ok(AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: vec![(
                "TDDY_ACP_SCENARIO".to_string(),
                scenario.display().to_string(),
            )],
            cwd: Some(launch.worktree.clone()),
        })
    }

    /// Not a managed session: the agent is handed no MCP exec tools.
    async fn mcp_servers_for(
        &self,
        _launch: &CodingAgentLaunch,
    ) -> Result<Vec<tddy_core::McpServerSpec>, SubagentError> {
        Ok(Vec::new())
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A daemon serving one session whose worktree is a git checkout, offering one coding agent.
struct ASession {
    service: TestDaemon,
    worktree: PathBuf,
    launcher: Arc<WritingLauncher>,
    _sessions: tempfile::TempDir,
}

fn a_session() -> ASession {
    let sessions = tempfile::tempdir().expect("sessions");
    let agents = sessions.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    std::fs::write(
        agents.join(format!("{CODEX}.yaml")),
        format!("name: {CODEX}\nharness: codex-acp\nmodel: stub-model\n"),
    )
    .expect("profile");
    let worktree = sessions.path().join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    git(&worktree, &["init", "-q", "-b", "main"]);
    std::fs::write(worktree.join("README.md"), "hello\n").expect("seed file");
    git(&worktree, &["add", "."]);
    git(
        &worktree,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let mut metadata: SessionMetadata = serde_json::from_value(serde_json::json!({
        "session_id": SESSION_ID,
        "project_id": "project-with-routing",
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");
    let launcher = Arc::new(WritingLauncher {
        dir: sessions.path().join("launcher"),
        launches: Mutex::new(Vec::new()),
    });
    std::fs::create_dir_all(&launcher.dir).expect("launcher dir");
    let service =
        test_service_with_coding_agent_launcher(sessions.path().to_path_buf(), launcher.clone());
    ASession {
        service,
        worktree,
        launcher,
        _sessions: sessions,
    }
}

impl ASession {
    async fn agent_id(&self) -> String {
        self.service
            .list_subagents(Request::direct(ListSubagentsRequest {}))
            .await
            .expect("ListSubagents")
            .into_inner()
            .subagents
            .into_iter()
            .find(|row| row.name == CODEX)
            .unwrap_or_else(|| panic!("a '{CODEX}' row"))
            .agent_id
    }

    async fn attach(&self, agent_id: &str, placement: AgentPlacement) {
        self.service
            .session_agents_service()
            .attach_session_agent(Request::direct(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                placement: placement as i32,
            }))
            .await
            .expect("attach");
    }

    async fn open(&self, agent_id: &str, conversation_id: &str) {
        self.service
            .session_agents_service()
            .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
                conversation_id: conversation_id.to_string(),
                system_prompt: String::new(),
            }))
            .await
            .expect("open");
    }

    /// Prompt the conversation and read its turn to the end.
    async fn prompt(&self, conversation_id: &str, prompt: &str) -> String {
        let mut stream = self
            .service
            .session_agents_service()
            .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
                prompt: prompt.to_string(),
                max_turns: None,
                yield_conditions_json: String::new(),
            }))
            .await
            .expect("prompting succeeds")
            .into_inner();
        let mut text = String::new();
        while let Some(chunk) = tokio::time::timeout(TURN_DEADLINE, stream.next())
            .await
            .expect("a chunk in time")
        {
            let chunk = chunk.expect("a chunk, not an error");
            text.push_str(&chunk.content_chunk);
            if chunk.last {
                break;
            }
        }
        text
    }

    /// The `ConversationWorktree` answer for `op` on `conversation_id`, parsed.
    async fn worktree_op(
        &self,
        conversation_id: &str,
        op: Op,
    ) -> Result<serde_json::Value, tddy_rpc::Status> {
        let response = self
            .service
            .conversation_worktree(Request::direct(ConversationWorktreeRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
                op: Some(op),
            }))
            .await?;
        Ok(serde_json::from_str(&response.get_ref().result_json).expect("result JSON"))
    }

    fn the_conversation_root(&self) -> PathBuf {
        self.launcher.launches.lock().expect("launches")[0]
            .worktree
            .clone()
    }

    fn the_conversation_branch(&self) -> String {
        format!("tddy/subagent/{SESSION_ID}/conv-route")
    }
}

/// A clone conversation whose agent has written [`WRITTEN`] on its first turn.
async fn a_clone_conversation_that_wrote_a_note() -> ASession {
    let session = a_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Clone).await;
    session.open(&agent, "conv-route").await;
    session.prompt("conv-route", "write a note").await;
    session
}

#[tokio::test]
#[serial]
async fn the_diff_of_a_clone_conversation_shows_what_its_agent_wrote() {
    // Given
    let session = a_clone_conversation_that_wrote_a_note().await;

    // When
    let answer = session
        .worktree_op(
            "conv-route",
            Op::Diff(DiffOp {
                from: String::new(),
                to: String::new(),
            }),
        )
        .await
        .expect("the diff is answered");

    // Then
    assert_eq!(
        (
            answer["diff"]["files"].clone(),
            answer["diff"]["diff"]
                .as_str()
                .map(|text| text.contains(WRITTEN_CONTENTS.trim_end())),
        ),
        (
            serde_json::json!({ "created": 1, "updated": 0, "removed": 0 }),
            Some(true),
        )
    );
}

#[tokio::test]
#[serial]
async fn pulling_a_clone_conversation_brings_its_work_into_the_session_worktree() {
    // Given
    let session = a_clone_conversation_that_wrote_a_note().await;
    let before = session.worktree.join(WRITTEN).exists();

    // When
    let answer = session
        .worktree_op(
            "conv-route",
            Op::PullRange(PullRangeOp {
                from: String::new(),
                to: String::new(),
                already_pulled: Vec::new(),
            }),
        )
        .await
        .expect("the pull is answered");

    // Then
    let pulled_commits = answer["pulled"]["commits"]
        .as_array()
        .map(|commits| commits.len());
    assert_eq!(
        (
            before,
            pulled_commits,
            std::fs::read_to_string(session.worktree.join(WRITTEN)).ok(),
        ),
        (false, Some(1), Some(WRITTEN_CONTENTS.to_string()))
    );
}

#[tokio::test]
#[serial]
async fn ending_a_clone_conversation_removes_its_worktree_and_its_branch() {
    // Given
    let session = a_clone_conversation_that_wrote_a_note().await;
    let root = session.the_conversation_root();
    let branch = session.the_conversation_branch();
    let branch_before = git(&session.worktree, &["branch", "--list", &branch]);

    // When
    let answer = session
        .worktree_op("conv-route", Op::Remove(RemoveOp {}))
        .await
        .expect("the remove is answered");

    // Then
    assert_eq!(
        (
            branch_before.is_empty(),
            answer["removed"].clone(),
            root.exists(),
            git(&session.worktree, &["branch", "--list", &branch]).is_empty(),
        ),
        (false, serde_json::json!(true), false, true)
    );
}

#[tokio::test]
#[serial]
async fn a_shared_conversation_has_no_worktree_to_diff_pull_or_end() {
    // Given
    let session = a_session();
    let agent = session.agent_id().await;
    session.attach(&agent, AgentPlacement::Shared).await;
    session.open(&agent, "conv-shared").await;
    session.prompt("conv-shared", "write a note").await;

    // When
    let diff = session
        .worktree_op(
            "conv-shared",
            Op::Diff(DiffOp {
                from: String::new(),
                to: String::new(),
            }),
        )
        .await
        .expect_err("there is no worktree to diff");
    let pull = session
        .worktree_op(
            "conv-shared",
            Op::PullRange(PullRangeOp {
                from: String::new(),
                to: String::new(),
                already_pulled: Vec::new(),
            }),
        )
        .await
        .expect("the pull is answered");
    let end = session
        .worktree_op("conv-shared", Op::Remove(RemoveOp {}))
        .await
        .expect("the remove is answered");

    // Then
    assert_eq!(
        (
            diff.code(),
            pull["pulled"].clone(),
            end["removed"].clone(),
            std::fs::read_to_string(session.worktree.join(WRITTEN)).ok(),
        ),
        (
            Code::FailedPrecondition,
            serde_json::Value::Null,
            serde_json::json!(false),
            Some(WRITTEN_CONTENTS.to_string()),
        )
    );
}
