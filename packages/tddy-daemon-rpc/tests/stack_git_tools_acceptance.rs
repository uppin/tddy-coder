//! Acceptance: the PR-stack git tools are daemon exec tools that run git **on the daemon, in each
//! node's own worktree** (`#orchestrators` 10/11). Main — a read-only session with no checkout of
//! the nodes' branches — merges, closes and repoints a node's PR; `pr_set_parents` rebases a node in
//! its own worktree and pushes it to the project's remote; a node whose worktree is dirty or whose
//! session is running is refused untouched; `pr_resolve_conflicts { delegate }` asks the node's
//! session to resolve the paths — a running one by a new turn, a stopped one resumed with it;
//! `pr_spawn_child` starts the node's session linked to main; and all six act on a stack another
//! host holds through forwarding.
//!
//! Each host's project points at a real repository whose only remote is a bare repository named
//! `upstream` (no `origin`), so a push that lands proves it went to the project's remote. A node's
//! worktree is a real session's worktree. Stacks are the project's records (`ProjectStackStore`),
//! read back through the real store. GitHub is a recording stand-in attached to the daemon
//! (`RpcHandlers::attach_stack_github`); the project is a Local gh one, so its account's token is
//! what the host's `gh` stub answers. A running session's agent is a recording stand-in on its main
//! terminal and a stopped one's is a stub `claude` the host really resumes, as the
//! `SendSessionMessage` / `ResumeSession` suite has them (`#orchestrators` 4/11). Cross-host calls go
//! through `support`'s in-memory peer forwarder, never LiveKit.
//!
//! Sequencing: every test here is served through `#orchestrators` 7/11's caller classification
//! (`stack_caller_of`) and `stack_id` addressing (`addressed_stack_id`), and the cross-host test
//! through its forwarding — they pass only once that node is green as well as this one.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-git-tools.md
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-git-tools.md

mod support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use support::{
    a_host_as_current_user_with_gh_at, a_host_running_claude_as_with_gh_at, a_workspace_in, git,
    git_stdout, session_started_by, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT,
};
use tddy_core::changeset::{read_changeset, write_changeset, GithubPrStatus, Stack, StackNode};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::WorkflowError;
use tddy_daemon_rpc::exec_tool::stack_tools::{StackGithub, StackGithubClients};
use tddy_github::pr_api::{
    CheckRun, GithubPrApi, GithubPrInsightApi, PrDetail, PrFile, PrIssueComment, PrLookupOutcome,
    PrRef, PrReview, PrReviewComment, PrSearchHit, PrSearchQuery,
};
use tddy_projects::project_storage::{read_projects, write_projects};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::gh_cli::a_gh_cli;
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};
use tddy_tool_engine::EXEC_TOOL_SERVICE;

/// The token the project's account resolves to: what the host's `gh` stub answers.
const PROJECTS_ACCOUNT_TOKEN: &str = "gho_the_projects_account";
const REPO: &str = "acme/alpha";
/// The project's only remote. There is no `origin`.
const PROJECT_REMOTE: &str = "upstream";
const DEFAULT_BRANCH: &str = "main";
/// The file a node's branch and its base both change, differently.
const CONFLICTING_FILE: &str = "shared.txt";
const TEST_MODEL: &str = "claude-opus-4-8";
/// How long a stand-in agent is given to start, and a typed byte or a started session to arrive.
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
/// What a stand-in agent prints once its terminal is raw and it is recording.
const READY_MARKER: &str = "READY";

// ── GitHub ───────────────────────────────────────────────────────────────────────────────────

/// What the stand-in GitHub was asked: every client built as `(repo, token)`, every merge, close
/// and base change.
#[derive(Default)]
struct GithubRecord {
    clients: Vec<(String, String)>,
    merged: Vec<u64>,
    closed: Vec<u64>,
    rebased: Vec<(u64, String)>,
    /// The open pull request of each head branch.
    open_prs: HashMap<String, u64>,
}

/// GitHub as the stack tools reach it: a branch's open pull request is the one a test opened on it,
/// and a merge, a close and a base change are recorded and accepted.
#[derive(Clone, Default)]
struct RecordingGithub(Arc<Mutex<GithubRecord>>);

impl RecordingGithub {
    fn has_an_open_pr(&self, branch: &str, number: u64) {
        self.0
            .lock()
            .unwrap()
            .open_prs
            .insert(branch.to_string(), number);
    }
    fn clients_built(&self) -> Vec<(String, String)> {
        self.0.lock().unwrap().clients.clone()
    }
    fn merged(&self) -> Vec<u64> {
        self.0.lock().unwrap().merged.clone()
    }
    fn closed(&self) -> Vec<u64> {
        self.0.lock().unwrap().closed.clone()
    }
    fn base_changes(&self) -> Vec<(u64, String)> {
        self.0.lock().unwrap().rebased.clone()
    }
}

impl StackGithubClients for RecordingGithub {
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn StackGithub> {
        self.0
            .lock()
            .unwrap()
            .clients
            .push((repo.to_string(), token.to_string()));
        Box::new(self.clone())
    }
}

fn not_served(what: &str) -> WorkflowError {
    WorkflowError::WriteFailed(format!("the stand-in GitHub serves no {what}"))
}

impl GithubPrApi for RecordingGithub {
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, WorkflowError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .open_prs
            .get(head_branch)
            .map(|number| PrRef {
                number: *number,
                head_sha: String::new(),
                base_branch: String::new(),
                url: format!("https://github.com/{REPO}/pull/{number}"),
            }))
    }
    fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
        PrLookupOutcome::NotFound
    }
    fn merge_pr(&self, number: u64) -> Result<String, WorkflowError> {
        self.0.lock().unwrap().merged.push(number);
        Ok(format!("merge-sha-{number}"))
    }
    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), WorkflowError> {
        self.0
            .lock()
            .unwrap()
            .rebased
            .push((number, new_base.to_string()));
        Ok(())
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, WorkflowError> {
        Err(not_served("PR creation"))
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), WorkflowError> {
        Ok(())
    }
    fn close_pr(&self, number: u64) -> Result<(), WorkflowError> {
        self.0.lock().unwrap().closed.push(number);
        Ok(())
    }
}

impl GithubPrInsightApi for RecordingGithub {
    fn get_pr(&self, _number: u64) -> Result<PrDetail, WorkflowError> {
        Err(not_served("PR read"))
    }
    fn list_pr_files(&self, _number: u64) -> Result<Vec<PrFile>, WorkflowError> {
        Err(not_served("file list"))
    }
    fn list_check_runs(&self, _head_sha: &str) -> Result<Vec<CheckRun>, WorkflowError> {
        Err(not_served("check runs"))
    }
    fn list_reviews(&self, _number: u64) -> Result<Vec<PrReview>, WorkflowError> {
        Err(not_served("reviews"))
    }
    fn list_review_comments(&self, _number: u64) -> Result<Vec<PrReviewComment>, WorkflowError> {
        Err(not_served("review comments"))
    }
    fn list_issue_comments(&self, _number: u64) -> Result<Vec<PrIssueComment>, WorkflowError> {
        Err(not_served("conversation"))
    }
    fn search_prs(&self, _query: &PrSearchQuery) -> Result<Vec<PrSearchHit>, WorkflowError> {
        Err(not_served("search"))
    }
    fn patch_pr_title_body(
        &self,
        _number: u64,
        _title: Option<&str>,
        _body: Option<&str>,
    ) -> Result<(), WorkflowError> {
        Err(not_served("title/body edit"))
    }
}

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Two hosts of one project, each reaching the stand-in GitHub, each project repository pushing to
/// its own bare `upstream`, with main on host-a.
struct World {
    a: Host,
    b: Host,
    peers: Arc<InProcessPeers>,
    github: RecordingGithub,
    main: String,
    /// The bare `upstream` of host-a's and host-b's project repository.
    remotes: (tempfile::TempDir, tempfile::TempDir),
    _gh: tempfile::TempDir,
    _claude: Option<AClaudeStub>,
}

fn a_gh_answering_the_projects_account() -> (tempfile::TempDir, PathBuf) {
    let gh_dir = tempfile::tempdir().unwrap();
    let gh = a_gh_cli(gh_dir.path())
        .logged_in_as("acme-bot", 7)
        .with_token(PROJECTS_ACCOUNT_TOKEN)
        .build();
    (gh_dir, gh)
}

async fn a_world() -> World {
    let (gh_dir, gh) = a_gh_answering_the_projects_account();
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_as_current_user_with_gh_at(HOST_A, &peers, &gh);
    let b = a_host_as_current_user_with_gh_at(HOST_B, &peers, &gh);
    a_world_of(a, b, peers, gh_dir, None).await
}

/// [`a_world`] whose host-a resumes `claude-cli` sessions with a stub `claude` it really spawns.
async fn a_world_resuming_claude() -> World {
    let (gh_dir, gh) = a_gh_answering_the_projects_account();
    let claude = AClaudeStub::new();
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host_running_claude_as_with_gh_at(HOST_A, &peers, &claude.binary(), &gh);
    let b = a_host_as_current_user_with_gh_at(HOST_B, &peers, &gh);
    a_world_of(a, b, peers, gh_dir, Some(claude)).await
}

async fn a_world_of(
    a: Host,
    b: Host,
    peers: Arc<InProcessPeers>,
    gh: tempfile::TempDir,
    claude: Option<AClaudeStub>,
) -> World {
    let github = RecordingGithub::default();
    let remote_a = a.pushes_only_to_a_bare_upstream();
    let remote_b = b.pushes_only_to_a_bare_upstream();
    for host in [&a, &b] {
        host.project_is_on_github_acting_as_the_hosts_gh();
        host.daemon
            .handlers()
            .attach_stack_github(Arc::new(github.clone()));
    }
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    World {
        a,
        b,
        peers,
        github,
        main,
        remotes: (remote_a, remote_b),
        _gh: gh,
        _claude: claude,
    }
}

impl World {
    fn remote_of(&self, host: &Host) -> &Path {
        if std::ptr::eq(host, &self.a) {
            self.remotes.0.path()
        } else {
            self.remotes.1.path()
        }
    }

    /// The tip of `branch` on `host`'s project remote, if it has one.
    fn remote_tip(&self, host: &Host, branch: &str) -> String {
        git_stdout(
            self.remote_of(host),
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
    }

    /// The project's default branch moves on by one commit on `host`'s remote — one changing
    /// `file` to `contents` — so a node forked before it is behind its base.
    fn the_default_branch_moves_on(&self, host: &Host, file: &str, contents: &str) {
        let repo = host.project_repo();
        git(&repo, &["checkout", "-q", DEFAULT_BRANCH]);
        std::fs::write(repo.join(file), contents).unwrap();
        git(&repo, &["add", file]);
        git(&repo, &["commit", "-q", "-m", &format!("base: {file}")]);
        git(&repo, &["push", "-q", PROJECT_REMOTE, DEFAULT_BRANCH]);
        git(&repo, &["fetch", "-q", PROJECT_REMOTE]);
    }

    /// Call `tool` as main on host-a.
    async fn main_calls(&self, tool: &str, args: Value) -> Answer {
        call(&self.a, &self.main, tool, args).await
    }
}

impl Host {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    fn stacks(&self) -> ProjectStackStore {
        ProjectStackStore::new(self.sessions.path().join("projects"))
    }

    /// The project's main repository on this host: the checkout its session worktrees hang off.
    fn project_repo(&self) -> PathBuf {
        read_projects(&self.sessions.path().join("projects"))
            .unwrap()
            .into_iter()
            .find(|row| row.project_id == THIS_PROJECT)
            .map(|row| PathBuf::from(row.main_repo_path))
            .expect("the project is registered")
    }

    /// The project's repository loses `origin` and gains a bare `upstream`, which the project's row
    /// names as its remote. Answers the bare repository.
    fn pushes_only_to_a_bare_upstream(&self) -> tempfile::TempDir {
        let bare = tempfile::tempdir().unwrap();
        git(bare.path(), &["init", "-q", "--bare", "-b", DEFAULT_BRANCH]);
        let repo = self.project_repo();
        git(&repo, &["remote", "remove", "origin"]);
        git(
            &repo,
            &[
                "remote",
                "add",
                PROJECT_REMOTE,
                bare.path().to_str().unwrap(),
            ],
        );
        git(&repo, &["push", "-q", "-u", PROJECT_REMOTE, DEFAULT_BRANCH]);
        git(&repo, &["fetch", "-q", PROJECT_REMOTE]);
        self.edit_project_row(|row| {
            row.remote_name = Some(PROJECT_REMOTE.to_string());
            row.main_branch_ref = Some(format!("{PROJECT_REMOTE}/{DEFAULT_BRANCH}"));
        });
        bare
    }

    /// The project's row names the GitHub repository and acts as the host's `gh` login.
    fn project_is_on_github_acting_as_the_hosts_gh(&self) {
        self.edit_project_row(|row| {
            row.git_url = format!("https://github.com/{REPO}.git");
            row.local_gh = true;
        });
    }

    fn edit_project_row(&self, edit: impl Fn(&mut tddy_projects::project_storage::ProjectData)) {
        let projects_dir = self.sessions.path().join("projects");
        let rows = read_projects(&projects_dir)
            .unwrap()
            .into_iter()
            .map(|mut row| {
                if row.project_id == THIS_PROJECT {
                    edit(&mut row);
                }
                row
            })
            .collect::<Vec<_>>();
        write_projects(&projects_dir, &rows).unwrap();
    }

    /// A stack of the project held by this host, planned by `created_by`, its nodes `planned`.
    fn holds_a_stack(&self, created_by: &str, planned: Vec<StackNode>) -> String {
        self.stacks()
            .create(
                THIS_PROJECT,
                created_by,
                Stack {
                    version: 1,
                    nodes: planned,
                },
            )
            .unwrap()
            .stack_id
    }

    fn node_of(&self, stack_id: &str, node_id: &str) -> StackNode {
        self.stacks()
            .read(THIS_PROJECT, stack_id)
            .unwrap()
            .stack
            .nodes
            .into_iter()
            .find(|n| n.node_id == node_id)
            .expect("the node is planned")
    }

    fn update_node(&self, stack_id: &str, node_id: &str, edit: impl Fn(&mut StackNode)) {
        self.stacks()
            .update(THIS_PROJECT, stack_id, |stack| {
                let node = stack
                    .nodes
                    .iter_mut()
                    .find(|n| n.node_id == node_id)
                    .expect("the node is planned");
                edit(node);
            })
            .unwrap();
    }

    /// Bind `node_id` of `stack_id` to `session_id`, started by `orchestrator`: the node records the
    /// session and the branch its worktree holds, which gets one commit adding `<node_id>.txt` on
    /// top of `fork_from` (the default branch when `None`) and is pushed to the project's remote.
    /// Answers the node's worktree.
    fn binds_a_node_to(
        &self,
        orchestrator: &str,
        stack_id: &str,
        node_id: &str,
        session_id: &str,
        fork_from: Option<&str>,
    ) -> NodeWorktree {
        let dir = self.session_dir(session_id);
        let mut changeset = read_changeset(&dir).unwrap_or_default();
        changeset.orchestrator_session_id = Some(orchestrator.to_string());
        write_changeset(&dir, &changeset).unwrap();
        let worktree = self.worktree_of(session_id);
        let branch = git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let base = fork_from
            .map(str::to_string)
            .unwrap_or_else(|| format!("{PROJECT_REMOTE}/{DEFAULT_BRANCH}"));
        git(&worktree, &["reset", "-q", "--hard", &base]);
        let node = NodeWorktree {
            session_id: session_id.to_string(),
            branch: branch.clone(),
            path: worktree,
        };
        node.commits(&format!("{node_id}.txt"), node_id);
        git(&node.path, &["push", "-q", "-f", PROJECT_REMOTE, &branch]);
        self.update_node(stack_id, node_id, |n| {
            n.session_id = Some(session_id.to_string());
            n.branch = Some(branch.clone());
        });
        node
    }

    /// [`Self::binds_a_node_to`] a new workspace session of the project on this host.
    async fn a_node_session(
        &self,
        orchestrator: &str,
        stack_id: &str,
        node_id: &str,
        fork_from: Option<&str>,
    ) -> NodeWorktree {
        let session_id = a_workspace_in(&self.daemon, THIS_PROJECT).await;
        self.binds_a_node_to(orchestrator, stack_id, node_id, &session_id, fork_from)
    }

    /// `node_id` of `stack_id` records its open pull request, number `number`, which GitHub reports
    /// as the open pull request of the node's branch.
    fn node_has_an_open_pr(
        &self,
        github: &RecordingGithub,
        stack_id: &str,
        node_id: &str,
        number: u64,
    ) {
        self.update_node(stack_id, node_id, |n| {
            n.pr_status = Some(GithubPrStatus {
                phase: "open".to_string(),
                url: Some(format!("https://github.com/{REPO}/pull/{number}")),
                error: None,
            });
        });
        let branch = self
            .node_of(stack_id, node_id)
            .branch
            .expect("a node with a PR owns a branch");
        github.has_an_open_pr(&branch, number);
    }

    /// Record `session_id` as running: its process is alive.
    fn session_is_running(&self, session_id: &str, session_type: &str) {
        let dir = self.session_dir(session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_type = Some(session_type.to_string());
        meta.pid = Some(std::process::id());
        meta.status = "active".to_string();
        meta.activity_status = Some("Running".to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }

    /// Whether the session's changeset names `orchestrator` as its orchestrator.
    fn orchestrator_of(&self, session_id: &str) -> Option<String> {
        read_changeset(&self.session_dir(session_id))
            .ok()
            .and_then(|c| c.orchestrator_session_id)
    }
}

/// A node's worktree: a real session's, on the branch the node owns.
struct NodeWorktree {
    session_id: String,
    branch: String,
    path: PathBuf,
}

impl NodeWorktree {
    /// Commit `file` with `contents` on the node's branch.
    fn commits(&self, file: &str, contents: &str) {
        std::fs::write(self.path.join(file), contents).unwrap();
        git(&self.path, &["add", file]);
        git(
            &self.path,
            &["commit", "-q", "-m", &format!("node: {file}")],
        );
    }

    /// Commit a change to [`CONFLICTING_FILE`] the base does not have, and push it.
    fn changes_the_conflicting_file(&self) {
        self.commits(CONFLICTING_FILE, "the node's version\n");
        git(
            &self.path,
            &["push", "-q", "-f", PROJECT_REMOTE, &self.branch],
        );
    }

    fn head(&self) -> String {
        git_stdout(&self.path, &["rev-parse", "HEAD"])
    }

    fn status(&self) -> String {
        git_stdout(&self.path, &["status", "--porcelain"])
    }

    /// Whether `commit` is an ancestor of the node's branch.
    fn contains(&self, commit: &str) -> bool {
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", commit, "HEAD"])
            .current_dir(&self.path)
            .status()
            .unwrap()
            .success()
    }

    /// Whether a merge is left in progress in the worktree.
    fn has_a_merge_in_progress(&self) -> bool {
        let git_dir = git_stdout(&self.path, &["rev-parse", "--absolute-git-dir"]);
        Path::new(&git_dir).join("MERGE_HEAD").exists()
    }
}

/// A planned node with `parents`, no branch and no session.
fn planned(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("Node {node_id}"),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        ..StackNode::default()
    }
}

/// [`planned`], with a branch to be created when its session is started.
fn planned_on(node_id: &str, branch_suggestion: &str) -> StackNode {
    StackNode {
        branch_suggestion: Some(branch_suggestion.to_string()),
        ..planned(node_id, &[])
    }
}

fn phase_of(node: &StackNode) -> Option<String> {
    node.pr_status.as_ref().map(|s| s.phase.clone())
}

fn internal_status_of(node: &StackNode) -> Option<String> {
    node.internal_status.as_ref().map(|s| s.kind.clone())
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// What came back from a tool call as the agent sees it: an answer, or why there is none. A call
/// the daemon refuses outright is a refusal here too, so a missing tool reads as one.
struct Answer {
    refused: bool,
    text: String,
}

impl Answer {
    fn assert_answered(&self) -> Value {
        assert!(
            !self.refused,
            "expected an answer, was refused: {}",
            self.text
        );
        serde_json::from_str(&self.text).expect("the answer is JSON")
    }

    fn assert_refused_saying(&self, reason: &str) -> &Self {
        assert!(self.refused, "expected a refusal, got {}", self.text);
        assert!(
            self.text.contains(reason),
            "refused, but not naming {reason:?}: {}",
            self.text
        );
        self
    }
}

async fn call(host: &Host, caller: &str, tool: &str, args: Value) -> Answer {
    let result = host
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: caller.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await;
    match result {
        Ok(response) => {
            let response = response.into_inner();
            Answer {
                refused: response.is_error,
                text: if response.is_error {
                    response.error_message
                } else {
                    response.result_json
                },
            }
        }
        Err(status) => Answer {
            refused: true,
            text: format!("{:?}: {}", status.code(), status.message()),
        },
    }
}

/// The conflicting paths a `pr_resolve_conflicts` answer names.
fn conflicts_in(answer: &Value) -> Vec<String> {
    answer["conflicts"]
        .as_array()
        .expect("a conflicts answer lists conflicts")
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect()
}

// ── agents ───────────────────────────────────────────────────────────────────────────────────

/// The shell that puts a PTY in raw mode, turns bracketed paste on, announces itself and then
/// appends every byte it is typed to `record`.
fn a_recording_agent_script(record: &Path) -> String {
    format!(
        "stty raw -echo\nprintf '\\033[?2004h{READY_MARKER}\\r\\n'\nexec cat >> '{}'\n",
        record.display()
    )
}

/// A stand-in agent on a running session's main terminal, and the file holding every byte typed
/// into it.
struct RecordingAgent {
    _dir: tempfile::TempDir,
    record: PathBuf,
}

impl RecordingAgent {
    async fn on_the_main_terminal_of(host: &Host, session_id: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("typed.bin");
        let script = dir.path().join("stand_in_agent.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\n{}", a_recording_agent_script(&record)),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let handle = host
            .terminals
            .start_with_options(
                session_id,
                host.worktree_of(session_id),
                TEST_MODEL,
                vec![script.to_str().unwrap().to_string()],
                "claude-cli",
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("the main terminal starts");
        a_capture_showing(&handle.capture, READY_MARKER, PTY_STUB_OUTPUT).await;
        Self { _dir: dir, record }
    }

    async fn a_submitted_turn(&self) -> String {
        a_submitted_turn_in(&self.record).await
    }
}

/// What an agent recording into `record` was typed once a turn was submitted (Enter), as text.
async fn a_submitted_turn_in(record: &Path) -> String {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let typed = std::fs::read(record).unwrap_or_default();
        if typed.ends_with(b"\r") || tokio::time::Instant::now() >= deadline {
            return String::from_utf8_lossy(&typed).into_owned();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A stub `claude` the host really spawns: it appends its argv to `argv.txt` beside itself, then
/// records what it is typed into `typed.bin` beside itself.
struct AClaudeStub {
    dir: tempfile::TempDir,
}

impl AClaudeStub {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\n{}",
                dir.path().join("argv.txt").display(),
                a_recording_agent_script(&dir.path().join("typed.bin"))
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn binary(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    fn typed(&self) -> PathBuf {
        self.dir.path().join("typed.bin")
    }

    fn launches(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl World {
    fn claude(&self) -> &AClaudeStub {
        self._claude.as_ref().expect("this world resumes claude")
    }

    /// A claude-cli session main started on host-a and then terminated: a stopped session with a
    /// worktree and a conversation to resume.
    async fn a_stopped_claude_cli_session(&self) -> String {
        let started = self
            .main_calls(
                "StartSession",
                json!({ "session_type": "claude-cli", "model": TEST_MODEL }),
            )
            .await;
        started.assert_answered();
        let session = session_started_by(&self.a.daemon, &self.main, &started.text).await;
        let handle = self
            .a
            .terminals
            .get(&session)
            .await
            .expect("the session's agent was spawned");
        a_capture_showing(&handle.capture, READY_MARKER, PTY_STUB_OUTPUT).await;
        self.main_calls("TerminateSession", json!({ "session_id": session }))
            .await
            .assert_answered();
        self.until_listed_stopped(&session).await;
        session
    }

    async fn until_listed_stopped(&self, session: &str) {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let listed = self
                .main_calls("ListProjectSessions", json!({}))
                .await
                .assert_answered();
            let active = listed["sessions"]
                .as_array()
                .expect("a sessions array")
                .iter()
                .find(|row| row["session_id"] == session)
                .map(|row| row["is_active"] == true);
            if active == Some(false) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "session {session} is still listed as {listed}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Wait until `node_id` of `stack_id` on `host` is bound to a session, and answer that node.
    async fn until_the_node_is_bound(
        &self,
        host: &Host,
        stack_id: &str,
        node_id: &str,
    ) -> StackNode {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let node = host.node_of(stack_id, node_id);
            if node.session_id.is_some() || tokio::time::Instant::now() >= deadline {
                return node;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_merges_closes_and_repoints_a_nodes_pr_from_a_read_only_session() {
    // Given a stack on host-a: n1 and n2 each with an open PR, and n3 stacked on n1 with an open PR;
    // the default branch has moved on since n3 forked
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(
        &world.main,
        vec![
            planned("n1", &[]),
            planned("n2", &[]),
            planned("n3", &["n1"]),
        ],
    );
    let n1 = a.a_node_session(&world.main, &stack_id, "n1", None).await;
    a.a_node_session(&world.main, &stack_id, "n2", None).await;
    let n3 = a
        .a_node_session(&world.main, &stack_id, "n3", Some(&n1.branch))
        .await;
    a.node_has_an_open_pr(&world.github, &stack_id, "n1", 41);
    a.node_has_an_open_pr(&world.github, &stack_id, "n2", 42);
    a.node_has_an_open_pr(&world.github, &stack_id, "n3", 43);
    world.the_default_branch_moves_on(a, "base.txt", "moved on\n");
    let base_tip = world.remote_tip(a, DEFAULT_BRANCH);
    let checkout_head = git_stdout(&a.project_repo(), &["rev-parse", "--abbrev-ref", "HEAD"]);

    // When main merges n1's PR, closes n2's and repoints n3
    let merged = world
        .main_calls("pr_merge", json!({ "stack_id": stack_id, "node_id": "n1" }))
        .await;
    let closed = world
        .main_calls("pr_close", json!({ "stack_id": stack_id, "node_id": "n2" }))
        .await;
    let repointed = world
        .main_calls(
            "pr_repoint",
            json!({ "stack_id": stack_id, "node_id": "n3" }),
        )
        .await;

    // Then each was answered, and GitHub was asked as the project's account to merge #41, close #42
    // and retarget #43 onto the default branch ...
    merged.assert_answered();
    closed.assert_answered();
    repointed.assert_answered();
    assert_eq!(world.github.merged(), vec![41]);
    assert_eq!(world.github.closed(), vec![42]);
    assert_eq!(
        world.github.base_changes(),
        vec![(43, DEFAULT_BRANCH.to_string())]
    );
    assert!(world
        .github
        .clients_built()
        .iter()
        .all(|(repo, token)| repo == REPO && token == PROJECTS_ACCOUNT_TOKEN));
    // ... the plan records n1 merged and n2 closed ...
    assert_eq!(
        (
            phase_of(&a.node_of(&stack_id, "n1")),
            phase_of(&a.node_of(&stack_id, "n2"))
        ),
        (Some("merged".to_string()), Some("closed".to_string()))
    );
    // ... n3's branch was rebased onto the moved base in its own worktree and pushed to the
    // project's remote ...
    assert!(n3.contains(&base_tip), "n3 is rebased onto {base_tip}");
    assert_eq!(world.remote_tip(a, &n3.branch), n3.head());
    // ... and the project's checkout was not switched to any node's branch
    assert_eq!(
        git_stdout(&a.project_repo(), &["rev-parse", "--abbrev-ref", "HEAD"]),
        checkout_head
    );
}

#[tokio::test]
async fn set_parents_rebases_the_node_in_its_own_worktree_and_pushes_to_the_projects_remote() {
    // Given a stack on host-a whose n2 is stacked on n1, each in its own session's worktree, n2
    // with an open PR; the default branch has moved on since they forked
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(
        &world.main,
        vec![planned("n1", &[]), planned("n2", &["n1"])],
    );
    let n1 = a.a_node_session(&world.main, &stack_id, "n1", None).await;
    let n2 = a
        .a_node_session(&world.main, &stack_id, "n2", Some(&n1.branch))
        .await;
    a.node_has_an_open_pr(&world.github, &stack_id, "n2", 42);
    world.the_default_branch_moves_on(a, "base.txt", "moved on\n");
    let base_tip = world.remote_tip(a, DEFAULT_BRANCH);
    let checkout_head = git_stdout(&a.project_repo(), &["rev-parse", "--abbrev-ref", "HEAD"]);

    // When main makes n2 a root
    let moved = world
        .main_calls(
            "pr_set_parents",
            json!({ "stack_id": stack_id, "node_id": "n2", "parents": [] }),
        )
        .await;

    // Then the plan records n2 as a root ...
    assert_eq!(
        moved.assert_answered(),
        json!({ "node_id": "n2", "parents": [] })
    );
    assert_eq!(a.node_of(&stack_id, "n2").parents, Vec::<String>::new());
    // ... n2's own worktree now holds its branch rebased onto the moved base, its work kept ...
    assert!(n2.contains(&base_tip), "n2 is rebased onto {base_tip}");
    assert!(n2.path.join("n2.txt").exists(), "n2's own commit is kept");
    assert_eq!(n2.status(), "", "n2's worktree is left clean");
    // ... the rebased branch reached the project's remote, `upstream` ...
    assert_eq!(world.remote_tip(a, &n2.branch), n2.head());
    // ... its PR was retargeted onto the default branch ...
    assert_eq!(
        world.github.base_changes(),
        vec![(42, DEFAULT_BRANCH.to_string())]
    );
    // ... and the project's checkout was not switched to n2's branch
    assert_eq!(
        git_stdout(&a.project_repo(), &["rev-parse", "--abbrev-ref", "HEAD"]),
        checkout_head
    );
}

#[tokio::test]
async fn a_node_with_a_dirty_worktree_or_a_running_session_is_refused_untouched() {
    // Given a stack on host-a whose n2 and n3 are stacked on n1: n2's worktree has an uncommitted
    // file, n3's session is running; the default branch has moved on since they forked
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(
        &world.main,
        vec![
            planned("n1", &[]),
            planned("n2", &["n1"]),
            planned("n3", &["n1"]),
        ],
    );
    let n1 = a.a_node_session(&world.main, &stack_id, "n1", None).await;
    let n2 = a
        .a_node_session(&world.main, &stack_id, "n2", Some(&n1.branch))
        .await;
    let n3 = a
        .a_node_session(&world.main, &stack_id, "n3", Some(&n1.branch))
        .await;
    std::fs::write(n2.path.join("work-in-progress.txt"), "not committed\n").unwrap();
    a.session_is_running(&n3.session_id, "claude-cli");
    world.the_default_branch_moves_on(a, "base.txt", "moved on\n");
    let (n2_tip, n3_tip) = (n2.head(), n3.head());

    // When main makes n2 a root and repoints n3
    let moving_dirty = world
        .main_calls(
            "pr_set_parents",
            json!({ "stack_id": stack_id, "node_id": "n2", "parents": [] }),
        )
        .await;
    let repointing_running = world
        .main_calls(
            "pr_repoint",
            json!({ "stack_id": stack_id, "node_id": "n3" }),
        )
        .await;

    // Then each is refused naming why: n2's uncommitted file, n3's running session ...
    moving_dirty.assert_refused_saying("work-in-progress.txt");
    repointing_running.assert_refused_saying(&n3.session_id);
    // ... neither plan nor branch moved, locally or on the remote ...
    assert_eq!(a.node_of(&stack_id, "n2").parents, vec!["n1".to_string()]);
    assert_eq!((n2.head(), n3.head()), (n2_tip.clone(), n3_tip.clone()));
    assert_eq!(
        (
            world.remote_tip(a, &n2.branch),
            world.remote_tip(a, &n3.branch)
        ),
        (n2_tip, n3_tip)
    );
    // ... n2's uncommitted work is still there, and no PR was retargeted
    assert_eq!(
        std::fs::read_to_string(n2.path.join("work-in-progress.txt")).unwrap(),
        "not committed\n"
    );
    assert_eq!(world.github.base_changes(), Vec::new());
}

#[tokio::test]
async fn resolve_conflicts_with_delegate_asks_the_nodes_session_to_resolve_the_named_paths() {
    // Given a stack on host-a whose n1 changes a file its base also changed, differently; n1's
    // session is running, its agent a recording stand-in
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(&world.main, vec![planned("n1", &[])]);
    let n1 = a.a_node_session(&world.main, &stack_id, "n1", None).await;
    n1.changes_the_conflicting_file();
    world.the_default_branch_moves_on(a, CONFLICTING_FILE, "the base's version\n");
    a.session_is_running(&n1.session_id, "claude-cli");
    let agent = RecordingAgent::on_the_main_terminal_of(a, &n1.session_id).await;
    let n1_tip = n1.head();

    // When main asks for n1's conflicts, delegating their resolution
    let resolved = world
        .main_calls(
            "pr_resolve_conflicts",
            json!({ "stack_id": stack_id, "node_id": "n1", "delegate": true }),
        )
        .await;

    // Then it is answered with the conflicting path ...
    assert_eq!(
        conflicts_in(&resolved.assert_answered()),
        vec![CONFLICTING_FILE.to_string()]
    );
    // ... n1's agent was handed a new turn naming that path ...
    let turn = agent.a_submitted_turn().await;
    assert!(
        turn.contains(CONFLICTING_FILE),
        "the agent's turn names the conflicting path: {turn:?}"
    );
    // ... the plan marks n1 as having conflicts ...
    assert_eq!(
        internal_status_of(&a.node_of(&stack_id, "n1")).as_deref(),
        Some("has-conflicts")
    );
    // ... and the daemon left n1's worktree as it found it
    assert_eq!(n1.head(), n1_tip);
    assert!(
        !n1.has_a_merge_in_progress(),
        "no merge is left in progress"
    );
    assert_eq!(n1.status(), "", "n1's worktree is left clean");
}

#[tokio::test]
async fn resolve_conflicts_with_delegate_resumes_a_stopped_node_session_with_the_message() {
    // Given a stack on host-a whose n1 is a claude-cli session main started and then stopped, its
    // branch changing a file its base also changed, differently
    let world = a_world_resuming_claude().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(&world.main, vec![planned("n1", &[])]);
    let stopped = world.a_stopped_claude_cli_session().await;
    let n1 = a.binds_a_node_to(&world.main, &stack_id, "n1", &stopped, None);
    n1.changes_the_conflicting_file();
    world.the_default_branch_moves_on(a, CONFLICTING_FILE, "the base's version\n");

    // When main asks for n1's conflicts, delegating their resolution
    let resolved = world
        .main_calls(
            "pr_resolve_conflicts",
            json!({ "stack_id": stack_id, "node_id": "n1", "delegate": true }),
        )
        .await;

    // Then it is answered with the conflicting path ...
    assert_eq!(
        conflicts_in(&resolved.assert_answered()),
        vec![CONFLICTING_FILE.to_string()]
    );
    // ... n1's agent was launched again, resuming its own conversation ...
    let launches = world.claude().launches();
    assert_eq!(launches.len(), 2, "claude's launches: {launches:?}");
    assert!(
        launches[1].contains(&format!("--resume {stopped}")),
        "the second launch resumes the conversation: {}",
        launches[1]
    );
    // ... and its first turn names the conflicting path
    let turn = a_submitted_turn_in(&world.claude().typed()).await;
    assert!(
        turn.contains(CONFLICTING_FILE),
        "the resumed agent's first turn names the conflicting path: {turn:?}"
    );
}

#[tokio::test]
async fn spawn_child_from_main_starts_the_nodes_session_linked_to_main() {
    // Given a stack on host-a with a planned node n1 that owns no branch yet
    let world = a_world().await;
    let a = &world.a;
    let stack_id = a.holds_a_stack(&world.main, vec![planned_on("n1", "feature/alpha/n1")]);

    // When main starts n1's session
    let spawned = world
        .main_calls(
            "pr_spawn_child",
            json!({ "stack_id": stack_id, "node_id": "n1", "session_type": "workspace" }),
        )
        .await;

    // Then it is answered with the new session ...
    let answer = spawned.assert_answered();
    let child = answer["session_id"]
        .as_str()
        .expect("the answer names the started session")
        .to_string();
    // ... which the node is bound to, on its suggested branch ...
    let node = world.until_the_node_is_bound(a, &stack_id, "n1").await;
    assert_eq!(
        (node.session_id.as_deref(), node.branch.as_deref()),
        (Some(child.as_str()), Some("feature/alpha/n1"))
    );
    assert_eq!(
        git_stdout(
            &a.worktree_of(&child),
            &["rev-parse", "--abbrev-ref", "HEAD"]
        ),
        "feature/alpha/n1"
    );
    // ... and which records main as its orchestrator
    assert_eq!(a.orchestrator_of(&child), Some(world.main.clone()));
}

#[tokio::test]
async fn the_six_tools_act_on_a_stack_owned_by_another_host() {
    // Given a stack held by host-b, its node sessions and worktrees on host-b: n1 and n2 with open
    // PRs, n3 stacked on n1 with an open PR, n4 stacked on n3, n5 conflicting with its base, and a
    // planned n6; main is on host-a, where nothing of the stack is
    let world = a_world().await;
    let b = &world.b;
    let stack_id = b.holds_a_stack(
        &world.main,
        vec![
            planned("n1", &[]),
            planned("n2", &[]),
            planned("n3", &["n1"]),
            planned("n4", &["n3"]),
            planned("n5", &[]),
            planned_on("n6", "feature/alpha/n6"),
        ],
    );
    let n1 = b.a_node_session(&world.main, &stack_id, "n1", None).await;
    b.a_node_session(&world.main, &stack_id, "n2", None).await;
    let n3 = b
        .a_node_session(&world.main, &stack_id, "n3", Some(&n1.branch))
        .await;
    let n4 = b
        .a_node_session(&world.main, &stack_id, "n4", Some(&n3.branch))
        .await;
    let n5 = b.a_node_session(&world.main, &stack_id, "n5", None).await;
    n5.changes_the_conflicting_file();
    b.node_has_an_open_pr(&world.github, &stack_id, "n1", 41);
    b.node_has_an_open_pr(&world.github, &stack_id, "n2", 42);
    b.node_has_an_open_pr(&world.github, &stack_id, "n3", 43);
    world.the_default_branch_moves_on(b, CONFLICTING_FILE, "the base's version\n");
    let base_tip = world.remote_tip(b, DEFAULT_BRANCH);
    let on_b = |args: Value| {
        let mut args = args;
        args["stack_id"] = json!(stack_id);
        args["daemon_instance_id"] = json!(HOST_B);
        args
    };

    // When main runs each of the six on that stack, naming host-b
    let merged = world
        .main_calls("pr_merge", on_b(json!({ "node_id": "n1" })))
        .await;
    let closed = world
        .main_calls("pr_close", on_b(json!({ "node_id": "n2" })))
        .await;
    let repointed = world
        .main_calls("pr_repoint", on_b(json!({ "node_id": "n3" })))
        .await;
    let moved = world
        .main_calls(
            "pr_set_parents",
            on_b(json!({ "node_id": "n4", "parents": [] })),
        )
        .await;
    let conflicts = world
        .main_calls("pr_resolve_conflicts", on_b(json!({ "node_id": "n5" })))
        .await;
    let spawned = world
        .main_calls(
            "pr_spawn_child",
            on_b(json!({ "node_id": "n6", "session_type": "workspace" })),
        )
        .await;

    // Then every call was answered by host-b ...
    for answer in [&merged, &closed, &repointed, &moved, &conflicts, &spawned] {
        answer.assert_answered();
    }
    assert_eq!(
        world
            .peers
            .times_asked(HOST_B, EXEC_TOOL_SERVICE, "ExecuteTool"),
        6
    );
    // ... GitHub merged #41, closed #42 and retargeted #43 ...
    assert_eq!(
        (world.github.merged(), world.github.closed()),
        (vec![41], vec![42])
    );
    assert!(world
        .github
        .base_changes()
        .contains(&(43, DEFAULT_BRANCH.to_string())));
    // ... host-b's worktrees were rebased and pushed to host-b's remote ...
    assert!(n3.contains(&base_tip), "n3 is rebased onto {base_tip}");
    assert_eq!(world.remote_tip(b, &n3.branch), n3.head());
    assert!(n4.contains(&base_tip), "n4 is rebased onto {base_tip}");
    assert_eq!(b.node_of(&stack_id, "n4").parents, Vec::<String>::new());
    // ... host-b's plan records the merge, the close and n5's conflicts ...
    assert_eq!(
        (
            phase_of(&b.node_of(&stack_id, "n1")),
            phase_of(&b.node_of(&stack_id, "n2")),
            internal_status_of(&b.node_of(&stack_id, "n5")),
        ),
        (
            Some("merged".to_string()),
            Some("closed".to_string()),
            Some("has-conflicts".to_string())
        )
    );
    assert_eq!(
        conflicts_in(&conflicts.assert_answered()),
        vec![CONFLICTING_FILE.to_string()]
    );
    // ... n6's session was started on host-b, linked to main ...
    let child = spawned.assert_answered()["session_id"]
        .as_str()
        .expect("the answer names the started session")
        .to_string();
    let n6 = world.until_the_node_is_bound(b, &stack_id, "n6").await;
    assert_eq!(n6.session_id.as_deref(), Some(child.as_str()));
    assert_eq!(b.orchestrator_of(&child), Some(world.main.clone()));
    // ... and host-a holds no stack
    assert_eq!(world.a.stacks().list(THIS_PROJECT).unwrap(), Vec::new());
}
