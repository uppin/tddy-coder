//! Acceptance: a project-home session lists its project's worktrees and reads one, as the
//! Worktrees screen shows them, on its own host or another (`#start-parity` 3/9) — and clears,
//! deletes and restores one, as the screen's Clear, Delete and Restore do (`#orchestrators` 5/11;
//! their cross-host case is in `project_home_lifecycle_tools_acceptance.rs`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-worktree-list.md,
//! docs/dev/1-WIP/2026-11-10-session-lifecycle-tools.md

mod support;

use std::path::Path;
use std::process::{Child, Command};
use std::sync::Arc;

use support::{
    a_host, a_workspace_in, git, git_stdout, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT,
};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
    }
}

impl World {
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }
}

fn result_of(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON result")
}

fn listed_paths(listing: &serde_json::Value) -> Vec<String> {
    listing["worktrees"]
        .as_array()
        .expect("a worktrees array")
        .iter()
        .map(|w| w["path"].as_str().unwrap().to_string())
        .collect()
}

fn canonical(path: &std::path::Path) -> String {
    path.canonicalize().unwrap().display().to_string()
}

fn canonical_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .map(|p| canonical(std::path::Path::new(&p)))
        .collect()
}

#[tokio::test]
async fn list_worktrees_lists_the_projects_worktrees_on_this_host_with_their_branch() {
    // Given
    let world = a_world().await;

    // When
    let listing = result_of(
        &world
            .home_calls("ListWorktrees", serde_json::json!({}))
            .await,
    );

    // Then the project's session worktree on host-a is listed, with its branch, and counted
    let on_a = canonical(&world.a.worktree_of(&world.on_a));
    assert!(
        canonical_paths(listed_paths(&listing)).contains(&on_a),
        "{listing}"
    );
    assert_eq!(
        listing["total"].as_u64().unwrap() as usize,
        listed_paths(&listing).len()
    );
    let row = listing["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| canonical(std::path::Path::new(w["path"].as_str().unwrap())) == on_a)
        .unwrap();
    assert!(!row["branch_label"].as_str().unwrap().is_empty(), "{row}");
}

#[tokio::test]
async fn get_worktree_returns_the_row_the_list_holds_for_it() {
    // Given the row the list holds for the session's worktree
    let world = a_world().await;
    let listing = result_of(
        &world
            .home_calls("ListWorktrees", serde_json::json!({}))
            .await,
    );
    let on_a = canonical(&world.a.worktree_of(&world.on_a));
    let listed_row = listing["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| canonical(std::path::Path::new(w["path"].as_str().unwrap())) == on_a)
        .cloned()
        .expect("the worktree is listed");

    // When it is read on its own
    let detail = result_of(
        &world
            .home_calls(
                "GetWorktree",
                serde_json::json!({ "path": listed_row["path"] }),
            )
            .await,
    );

    // Then it is the same row
    assert_eq!(detail["worktree"], listed_row);
}

#[tokio::test]
async fn an_unknown_worktree_is_refused_naming_it() {
    // Given
    let world = a_world().await;

    // When
    let refused = world
        .home_calls(
            "GetWorktree",
            serde_json::json!({ "path": "/no/such/worktree" }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("/no/such/worktree"),
        "{}",
        refused.error_message
    );
}

#[tokio::test]
async fn list_worktrees_lists_another_hosts_worktrees() {
    // Given a session of the project on host-b
    let world = a_world().await;

    // When the home session lists host-b's worktrees
    let listing = result_of(
        &world
            .home_calls(
                "ListWorktrees",
                serde_json::json!({ "daemon_instance_id": HOST_B }),
            )
            .await,
    );

    // Then host-b's worktree is listed, and host-a's is not
    let paths = canonical_paths(listed_paths(&listing));
    assert!(paths.contains(&canonical(&world.b.worktree_of(&world.on_b))));
    assert!(!paths.contains(&canonical(&world.a.worktree_of(&world.on_a))));
}

// ── Clear, Delete and Restore (`#orchestrators` 5/11) ────────────────────────────────────────

/// A process standing in for a session's; killed when dropped, so a failing test leaks none.
struct StandIn(Child);

impl Drop for StandIn {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

/// A long-running process recorded as `session_id`'s on `host`: the session is active.
fn a_running_process_for(host: &Host, session_id: &str) -> StandIn {
    let child = Command::new("sleep").arg("600").spawn().unwrap();
    let dir =
        tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.pid = Some(child.id());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    StandIn(child)
}

impl World {
    /// The path `ListWorktrees` lists `session_id`'s worktree on host-a under.
    async fn listed_path_of(&self, session_id: &str) -> String {
        self.listed_path_for(&self.a.worktree_of(session_id)).await
    }

    /// The path `ListWorktrees` lists the worktree at `worktree` on host-a under.
    async fn listed_path_for(&self, worktree: &Path) -> String {
        let listing = result_of(
            &self
                .home_calls("ListWorktrees", serde_json::json!({}))
                .await,
        );
        let wanted = canonical(worktree);
        listed_paths(&listing)
            .into_iter()
            .find(|path| canonical(Path::new(path)) == wanted)
            .unwrap_or_else(|| panic!("{} is not listed: {listing}", worktree.display()))
    }

    /// The project's home worktree on host-a, made as `EnsureProjectHomeSession` makes it: the one
    /// checkout main and every orchestrator of the project share, whatever worktree the calling
    /// session records as its own.
    fn the_projects_home_worktree(&self) -> std::path::PathBuf {
        let main_checkout = main_checkout_of(&self.a.worktree_of(&self.on_a));
        tddy_worktree_service::home_worktree::ensure_home_worktree(
            &main_checkout,
            &tddy_core::worktree_dir(&main_checkout),
            THIS_PROJECT,
            "origin/main",
        )
        .expect("the project's home worktree is made")
    }

    /// `tool` from the home session on the worktree at `path`.
    async fn home_acts_on(&self, tool: &str, path: &str) -> ExecuteToolResponse {
        self.home_calls(tool, serde_json::json!({ "path": path }))
            .await
    }
}

/// The main checkout a worktree belongs to.
fn main_checkout_of(worktree: &Path) -> std::path::PathBuf {
    let common = git_stdout(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    Path::new(&common).parent().unwrap().to_path_buf()
}

/// The branches `main_checkout` holds.
fn branches_of(main_checkout: &Path) -> Vec<String> {
    git_stdout(main_checkout, &["branch", "--format=%(refname:short)"])
        .lines()
        .map(str::to_string)
        .collect()
}

/// The worktrees git lists for `main_checkout`, canonical.
fn git_worktrees_of(main_checkout: &Path) -> Vec<String> {
    git_stdout(main_checkout, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .filter_map(|path| Path::new(path).canonicalize().ok())
        .map(|path| path.display().to_string())
        .collect()
}

fn assert_refused_saying(response: &ExecuteToolResponse, reasons: &[&str]) {
    assert!(
        response.is_error,
        "expected a refusal, got {}",
        response.result_json
    );
    for reason in reasons {
        assert!(
            response.error_message.contains(reason),
            "refused, but not for {reason:?}: {}",
            response.error_message
        );
    }
}

#[tokio::test]
async fn delete_worktree_removes_a_clean_idle_worktree_and_keeps_its_branch() {
    // Given a stopped session's clean worktree, and its branch
    let world = a_world().await;
    let worktree = world.a.worktree_of(&world.on_a);
    let branch = git_stdout(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let main_checkout = main_checkout_of(&worktree);
    let path = world.listed_path_of(&world.on_a).await;

    // When the home session deletes the worktree
    let deleted = world.home_acts_on("DeleteWorktree", &path).await;

    // Then it is gone from disk and from git's worktree list, and its branch is kept
    result_of(&deleted);
    assert!(!worktree.exists(), "the worktree is still on disk");
    assert!(!git_worktrees_of(&main_checkout).contains(&canonical_or_raw(&worktree)));
    assert!(
        branches_of(&main_checkout).contains(&branch),
        "branch {branch} was not kept: {:?}",
        branches_of(&main_checkout)
    );
}

/// `path` canonical when it still exists, as given otherwise.
fn canonical_or_raw(path: &Path) -> String {
    path.canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[tokio::test]
async fn delete_worktree_refuses_a_dirty_worktree_naming_commit_and_stash() {
    // Given a stopped session whose worktree holds an uncommitted file
    let world = a_world().await;
    let worktree = world.a.worktree_of(&world.on_a);
    let uncommitted = worktree.join("notes.md");
    std::fs::write(&uncommitted, "work in progress\n").unwrap();
    let path = world.listed_path_of(&world.on_a).await;

    // When the home session deletes the worktree
    let refused = world.home_acts_on("DeleteWorktree", &path).await;

    // Then it is refused, told to commit or stash first, and the file is still there
    assert_refused_saying(&refused, &["CommitSessionWorktree", "StashSessionWorktree"]);
    assert!(uncommitted.exists(), "the uncommitted file was lost");
}

#[tokio::test]
async fn delete_worktree_refuses_the_home_worktree() {
    // Given the project's home worktree — a path of the project's, not the calling session's own
    // worktree, so the refusal holds for main and every orchestrator alike
    let world = a_world().await;
    let home_worktree = world.the_projects_home_worktree();
    assert_ne!(
        canonical(&home_worktree),
        canonical(&world.a.worktree_of(&world.home)),
        "the caller's own worktree must not be the home worktree here"
    );
    let path = world.listed_path_for(&home_worktree).await;

    // When the home session deletes it
    let refused = world.home_acts_on("DeleteWorktree", &path).await;

    // Then it is refused as the home worktree, naming its path, and stays
    assert_refused_saying(&refused, &["home worktree", &path]);
    assert!(home_worktree.is_dir(), "the home worktree was removed");
}

#[tokio::test]
async fn a_worktree_tool_is_refused_while_its_session_is_active() {
    // Given a session whose process runs, with a file in its worktree
    let world = a_world().await;
    let _process = a_running_process_for(&world.a, &world.on_a);
    let worktree = world.a.worktree_of(&world.on_a);
    let build_output = worktree.join("target.o");
    std::fs::write(&build_output, b"object code").unwrap();
    let path = world.listed_path_of(&world.on_a).await;

    // When the home session clears and deletes its worktree
    let clear = world.home_acts_on("ClearWorktree", &path).await;
    let delete = world.home_acts_on("DeleteWorktree", &path).await;

    // Then both are refused naming the active session, and the worktree is untouched
    assert_refused_saying(&clear, &[&world.on_a, "active"]);
    assert_refused_saying(&delete, &[&world.on_a, "active"]);
    assert!(
        build_output.exists(),
        "the active session's worktree was cleared"
    );
}

#[tokio::test]
async fn clear_worktree_and_restore_worktree_do_what_the_screen_does() {
    // Given a stopped session whose worktree holds a committed file and build output
    let world = a_world().await;
    let worktree = world.a.worktree_of(&world.on_a);
    std::fs::write(worktree.join("lib.rs"), "pub fn kept() {}\n").unwrap();
    git(&worktree, &["add", "lib.rs"]);
    git(&worktree, &["commit", "-q", "-m", "a kept file"]);
    std::fs::create_dir_all(worktree.join("target")).unwrap();
    std::fs::write(worktree.join("target/out.o"), b"object code").unwrap();
    let path = world.listed_path_of(&world.on_a).await;

    // When the home session clears it
    let cleared = world.home_acts_on("ClearWorktree", &path).await;

    // Then the build output is gone, and the committed file and the worktree stay (Clear)
    result_of(&cleared);
    assert!(
        !worktree.join("target").exists(),
        "the build output was not cleared"
    );
    assert!(
        worktree.join("lib.rs").exists(),
        "a committed file was cleared"
    );

    // Given the session's worktree was then removed
    let main_checkout = main_checkout_of(&worktree);
    git(
        &main_checkout,
        &["worktree", "remove", "--force", worktree.to_str().unwrap()],
    );

    // When the home session restores it, by the session's id
    let restored = world
        .home_calls(
            "RestoreWorktree",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // Then a worktree is cut again for the session and registered with git (Restore)
    let restored_at = result_of(&restored)["worktree_path"]
        .as_str()
        .expect("the restored worktree's path")
        .to_string();
    assert!(Path::new(&restored_at).is_dir(), "nothing at {restored_at}");
    assert!(git_worktrees_of(&main_checkout).contains(&canonical(Path::new(&restored_at))));
}
