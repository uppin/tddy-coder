use super::*;
use tddy_core::toolcall::ChildSpawnHandler;
use tddy_core::{Stack, StackNode};
use tddy_testing_commons::wait::eventually;
use tddy_workflow::SESSION_ATTACHMENTS_SUBDIR;

const VALID_TOKEN: &str = "stack-child-token";
const TEST_PROJECT_ID: &str = "stack-child-project";
const TEST_MODEL: &str = "claude-opus-4-8";
const ORCHESTRATOR_SESSION_ID: &str = "018f7777-cccc-7000-3333-000000000001";
const NODE_ID: &str = "n1";
const NODE_TITLE: &str = "Token store";
const NODE_DESCRIPTION: &str = "Adds the token store the rest of the stack reads.";

/// How long the stub gets to record the command line it was spawned with. A safety net, not a
/// prediction: it covers fork/exec and the daemon's own worktree setup under a loaded suite.
const AGENT_SPAWN: Duration = Duration::from_secs(10);

// ── The orchestrator ─────────────────────────────────────────────────────────────────────

/// A pr-stack orchestrator session on a daemon that can really start a child: a registered
/// project with a git repo, and a stub in place of `claude` that records its argv.
struct Orchestrator {
    service: ConnectionServiceImpl,
    config: DaemonConfig,
    os_user: String,
    sessions: tempfile::TempDir,
    agent_command_line_path: PathBuf,
    _repo: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_pr_stack_orchestrator() -> Orchestrator {
    let os_user = crate::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
        .expect("the test process's uid must resolve to a passwd entry");

    let repo = tempfile::tempdir().expect("repo dir");
    create_repo_with_origin(repo.path());

    let sessions = tempfile::tempdir().expect("sessions dir");
    register_project(&sessions.path().join("projects"), repo.path());

    let stub_dir = tempfile::tempdir().expect("stub dir");
    let agent_command_line_path = stub_dir.path().join("agent-command-line.txt");
    let stub = write_argv_recording_stub(stub_dir.path(), &agent_command_line_path);

    let config_dir = tempfile::tempdir().expect("config dir");
    let config_path = config_dir.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!(
            "users:\n  - github_user: \"{os_user}\"\n    os_user: \"{os_user}\"\nclaude_cli:\n  binary_path: {}\n",
            stub.display()
        ),
    )
    .expect("write daemon config");
    let config = DaemonConfig::load(&config_path).expect("daemon config must parse");

    let sessions_base = sessions.path().to_path_buf();
    let resolver: SessionsBaseResolver = Arc::new(move |_| Some(sessions_base.clone()));
    let resolved_user = os_user.clone();
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
    let service = ConnectionServiceImpl::new(
        config.clone(),
        resolver,
        sessions.path().to_path_buf(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    );

    let orchestrator = Orchestrator {
        service,
        config,
        os_user,
        sessions,
        agent_command_line_path,
        _repo: repo,
        _config_dir: config_dir,
        _stub_dir: stub_dir,
    };
    orchestrator.write_planned_stack();
    orchestrator
}

impl Orchestrator {
    fn sessions_base(&self) -> &Path {
        self.sessions.path()
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions_base(), session_id)
    }

    fn artifacts(&self) -> PathBuf {
        self.session_dir(ORCHESTRATOR_SESSION_ID).join("artifacts")
    }

    /// The planned stack and the session metadata a spawn reads: one undeveloped node, and the
    /// model the child inherits.
    fn write_planned_stack(&self) {
        let dir = self.session_dir(ORCHESTRATOR_SESSION_ID);
        std::fs::create_dir_all(dir.join("artifacts")).expect("orchestrator artifacts");
        std::fs::write(
            dir.join(".session.yaml"),
            format!(
                "session_id: {ORCHESTRATOR_SESSION_ID}\nproject_id: {TEST_PROJECT_ID}\n\
                     created_at: 2026-08-29T10:00:00Z\nupdated_at: 2026-08-29T10:00:00Z\n\
                     status: active\nsession_type: claude-cli\nrecipe: pr-stack\nmodel: {TEST_MODEL}\n"
            ),
        )
        .expect("write orchestrator metadata");
        tddy_core::write_changeset(
            &dir,
            &Changeset {
                stack: Some(Stack {
                    version: 1,
                    nodes: vec![StackNode {
                        node_id: NODE_ID.to_string(),
                        title: NODE_TITLE.to_string(),
                        description: NODE_DESCRIPTION.to_string(),
                        branch_suggestion: Some("feature/token-store".to_string()),
                        ..Default::default()
                    }],
                }),
                ..Changeset::default()
            },
        )
        .expect("write orchestrator changeset");
    }

    /// The stack-level documents every child is offered.
    fn with_shared_documents(self) -> Self {
        std::fs::write(self.artifacts().join("pr-stack-plan.md"), "# The stack\n")
            .expect("write plan");
        std::fs::write(self.artifacts().join("exploration.md"), "# The map\n")
            .expect("write exploration map");
        self
    }

    /// The pair the `write-stack-docs` pass authors for one node.
    fn with_documents_for(self, node_id: &str) -> Self {
        let node_dir = self.artifacts().join("prs").join(node_id);
        std::fs::create_dir_all(&node_dir).expect("node docs dir");
        std::fs::write(
            node_dir.join("PRD.md"),
            format!("# {node_id} — what it delivers\n"),
        )
        .expect("write prd");
        std::fs::write(
            node_dir.join("changeset.md"),
            format!("# {node_id} — where the edges are\n"),
        )
        .expect("write changeset");
        self
    }

    /// The handler the `pr_spawn_child` tool reaches, wired exactly as
    /// [`ConnectionServiceImpl::start_claude_cli_session`] wires it for a pr-stack session.
    fn child_spawn_handler(&self) -> StackChildSpawnHandler {
        StackChildSpawnHandler {
            // Same clone production passes: the orchestrator is a session of this daemon, so
            // resolving a child's base never leaves the host.
            stack_parent_host: Arc::new(self.service.clone()),
            service: self.service.clone(),
            config: self.config.clone(),
            tddy_data_dir: self.sessions_base().to_path_buf(),
            claude_cli_manager: Arc::clone(&self.service.claude_cli_manager),
            os_user: self.os_user.clone(),
            project_id: TEST_PROJECT_ID.to_string(),
            sessions_base: self.sessions_base().to_path_buf(),
            orchestrator_session_id: ORCHESTRATOR_SESSION_ID.to_string(),
            orchestrator_session_dir: self.session_dir(ORCHESTRATOR_SESSION_ID),
        }
    }

    /// What the operator's Start-session dialog sends for this node: the same documents, as
    /// pre-populated attachment rows, plus the node's own brief as the initial prompt.
    fn dialog_start_request(&self) -> StartSessionRequest {
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: "new_branch_from_base".to_string(),
            new_branch_name: "feature/token-store".to_string(),
            initial_prompt: format!("{NODE_TITLE}\n\n{NODE_DESCRIPTION}"),
            stack_parent: ORCHESTRATOR_SESSION_ID.to_string(),
            attachments: crate::stack_doc_attachments::stack_doc_attachments(
                &self.session_dir(ORCHESTRATOR_SESSION_ID),
                ORCHESTRATOR_SESSION_ID,
                &local_instance_id_for_config(&self.config),
                NODE_ID,
            ),
            ..Default::default()
        }
    }

    async fn start_from_the_dialog(&self) -> String {
        self.service
            .start_session(Request::new(self.dialog_start_request()))
            .await
            .expect("the dialog's StartSession must succeed")
            .into_inner()
            .session_id
    }

    /// The command line the child's agent was actually spawned with, once the stub has recorded
    /// it. Read off disk rather than off the PTY: a terminal capture wraps at the window width,
    /// which would split the very sentence under test.
    async fn the_agent_was_spawned_with(&self) -> String {
        let path = self.agent_command_line_path.clone();
        eventually(
            "the child's agent to record its command line",
            AGENT_SPAWN,
            || {
                std::fs::read_to_string(&path)
                    .map_err(|e| format!("the stub has recorded nothing yet: {e}"))
            },
        )
        .await
    }

    /// What landed in the child's attachment store, by basename.
    fn documents_held_by(&self, session_id: &str) -> Vec<String> {
        let dir = self
            .session_dir(session_id)
            .join("artifacts")
            .join(SESSION_ATTACHMENTS_SUBDIR);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────────────────────

fn create_repo_with_origin(dir: &Path) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git command must run");
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "t@t.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
    run(&["remote", "add", "origin", dir.to_str().expect("repo path")]);
    run(&["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo: &Path) {
    std::fs::create_dir_all(projects_dir).expect("projects dir");
    std::fs::write(
        projects_dir.join("projects.yaml"),
        format!(
            "projects:\n  - project_id: {TEST_PROJECT_ID}\n    name: stack\n    git_url: \"\"\n    main_repo_path: {}\n",
            repo.display()
        ),
    )
    .expect("write projects.yaml");
}

/// A stand-in for `claude` that writes each argument it was given on its own line, so a
/// multi-line prompt is recoverable verbatim.
fn write_argv_recording_stub(dir: &Path, record_to: &Path) -> PathBuf {
    let script = dir.join("stub_claude.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            record_to.display()
        ),
    )
    .expect("write stub");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("stub must be executable");
    script
}

fn a_changeset_reading_instruction(command_line: &str) -> Option<&str> {
    command_line
        .lines()
        .find(|line| line.contains("Read your changeset at"))
}

// ── The agent's path: `pr_spawn_child` ───────────────────────────────────────────────────

#[tokio::test]
async fn a_child_the_agent_spawned_holds_the_nodes_documents() {
    // Given
    let orchestrator = a_pr_stack_orchestrator()
        .with_shared_documents()
        .with_documents_for(NODE_ID);

    // When
    let child = orchestrator
        .child_spawn_handler()
        .spawn_child(NODE_ID)
        .await
        .expect("spawning the planned node must succeed");

    // Then — materialized into the child's own attachment store, flat
    assert_eq!(
        orchestrator.documents_held_by(&child),
        vec![
            "PRD.md".to_string(),
            "changeset.md".to_string(),
            "exploration.md".to_string(),
            "pr-stack-plan.md".to_string(),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(
            orchestrator
                .session_dir(&child)
                .join("artifacts")
                .join(SESSION_ATTACHMENTS_SUBDIR)
                .join("changeset.md")
        )
        .expect("the child's changeset must be readable"),
        format!("# {NODE_ID} — where the edges are\n"),
        "the child must hold its own node's boundaries, byte for byte"
    );
}

#[tokio::test]
async fn a_child_the_agent_spawned_is_told_to_read_its_changeset() {
    // Given
    let orchestrator = a_pr_stack_orchestrator()
        .with_shared_documents()
        .with_documents_for(NODE_ID);

    // When
    orchestrator
        .child_spawn_handler()
        .spawn_child(NODE_ID)
        .await
        .expect("spawning the planned node must succeed");

    // Then — the node's brief, and where to read its boundaries before writing code
    let command_line = orchestrator.the_agent_was_spawned_with().await;
    assert!(
        command_line.contains(NODE_TITLE),
        "the child must be given its node's brief; got: {command_line:?}"
    );
    assert_eq!(
        a_changeset_reading_instruction(&command_line),
        Some("Read your changeset at artifacts/attachments/changeset.md before writing code — it states this PR's responsibility, its boundaries, and what each dependency delivers."),
        "the child must be pointed at the changeset it actually holds"
    );
}

// ── The operator's path: the Start-session dialog ────────────────────────────────────────

#[tokio::test]
async fn a_child_started_from_the_dialog_is_told_to_read_its_changeset() {
    // Given
    let orchestrator = a_pr_stack_orchestrator()
        .with_shared_documents()
        .with_documents_for(NODE_ID);

    // When
    let child = orchestrator.start_from_the_dialog().await;

    // Then — a child must not differ by how it was started
    assert_eq!(
        orchestrator.documents_held_by(&child),
        vec![
            "PRD.md".to_string(),
            "changeset.md".to_string(),
            "exploration.md".to_string(),
            "pr-stack-plan.md".to_string(),
        ]
    );
    let command_line = orchestrator.the_agent_was_spawned_with().await;
    assert_eq!(
        a_changeset_reading_instruction(&command_line),
        Some("Read your changeset at artifacts/attachments/changeset.md before writing code — it states this PR's responsibility, its boundaries, and what each dependency delivers."),
        "the dialog's child must be pointed at its changeset too"
    );
}

#[tokio::test]
async fn a_child_started_before_its_documents_were_written_is_told_nothing() {
    // Given — the docs pass has not reached this node; only the stack-level pair exists
    let orchestrator = a_pr_stack_orchestrator().with_shared_documents();

    // When
    let child = orchestrator.start_from_the_dialog().await;

    // Then — the shared documents still arrive, and nothing points at a file that is not there
    assert_eq!(
        orchestrator.documents_held_by(&child),
        vec!["exploration.md".to_string(), "pr-stack-plan.md".to_string()]
    );
    let command_line = orchestrator.the_agent_was_spawned_with().await;
    assert!(
        command_line.contains(NODE_TITLE),
        "the child must still be given its node's brief; got: {command_line:?}"
    );
    assert_eq!(
        a_changeset_reading_instruction(&command_line),
        None,
        "pointing at a changeset nobody wrote would send the agent hunting"
    );
}
