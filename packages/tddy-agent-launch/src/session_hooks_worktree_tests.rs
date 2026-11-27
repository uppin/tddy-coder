//! Unit tests: a worktree made ready for a launch's per-session hooks — on this host, and through a
//! remote shell (driven by `LocalShell`, which runs the same commands a target would).

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use super::*;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::claude_hooks::session_file::claude_session_hooks_file;
use tddy_core::claude_hooks::{build_claude_hooks_settings, HookCommandParams};

const THE_HOME_SESSION: &str = "01a16dc6-ab4c-7dc3-844e-baca4514f86e";
const AN_ORCHESTRATOR: &str = "01a1f0a7-11cd-7753-bb33-d733709924ef";

fn the_claude_spec() -> CodingAgentSpec {
    tddy_coding_agent_specs::embedded_spec("claude").expect("claude.yaml is built in")
}

/// A git checkout, standing for the project-home worktree.
fn a_checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary worktree");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["init", "--quiet"])
        .status()
        .expect("git runs");
    assert!(status.success(), "git init succeeds");
    dir
}

fn legacy_file(worktree: &Path) -> PathBuf {
    worktree.join(".claude/settings.local.json")
}

/// The `settings.local.json` an earlier daemon wrote for `session_id` into `worktree`, plus `extra`
/// top-level keys of the operator's.
fn the_daemon_wrote_shared_hooks_for(worktree: &Path, session_id: &str, extra: serde_json::Value) {
    let mut settings = build_claude_hooks_settings(&HookCommandParams {
        tddy_tools_path: "/usr/local/bin/tddy-tools",
        daemon_url: "http://127.0.0.1:8899",
        session_id,
        os_user: "ada",
        hook_token: "tok-orchestrator",
        daemon_socket: Some("/tmp/tddy-agent-tools-46362-0.sock"),
        answer_wait_secs: 120,
        remote_log: None,
        route_env: &[],
    });
    if let serde_json::Value::Object(extra) = extra {
        for (key, value) in extra {
            settings[key] = value;
        }
    }
    let path = legacy_file(worktree);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, settings.to_string()).unwrap();
}

/// The slots of a launch of `session_id` whose hooks file is its own, under `worktree`.
fn slots_with_the_hooks_of(worktree: &Path, session_id: &str) -> RenderedSlots {
    RenderedSlots {
        files: vec![(
            claude_session_hooks_file(worktree, session_id),
            b"{}".to_vec(),
        )],
        ..RenderedSlots::default()
    }
}

fn the_exclude_file_of(checkout: &Path) -> String {
    std::fs::read_to_string(checkout.join(".git/info/exclude")).unwrap_or_default()
}

fn on_a_target() -> PointerHost {
    PointerHost::Remote {
        shell: Arc::new(tddy_remote_shell::LocalShell::new()),
    }
}

#[tokio::test]
async fn the_daemons_shared_hooks_another_session_left_are_removed_before_the_launch() {
    // Given a worktree whose shared settings hold only an orchestrator's hooks
    let worktree = a_checkout();
    the_daemon_wrote_shared_hooks_for(worktree.path(), AN_ORCHESTRATOR, serde_json::json!({}));

    // When the home session's launch prepares it
    prepare_worktree_for_session_hooks(&tddy_fs::DiskFs, 
        &the_claude_spec(),
        worktree.path(),
        &slots_with_the_hooks_of(worktree.path(), THE_HOME_SESSION),
        &PointerHost::Local,
    )
    .await
    .expect("the worktree is prepared");

    // Then the shared file is gone, and every session's hooks directory is kept out of commits
    assert!(
        !legacy_file(worktree.path()).exists(),
        "the orchestrator's hooks would fire beside the home session's own"
    );
    assert!(
        the_exclude_file_of(worktree.path())
            .lines()
            .any(|l| l == "/.tddy/hooks/"),
        "info/exclude: {:?}",
        the_exclude_file_of(worktree.path())
    );
}

#[tokio::test]
async fn the_operators_own_settings_in_the_shared_file_are_kept() {
    // Given the daemon's hooks beside a permission rule Claude Code saved
    let worktree = a_checkout();
    the_daemon_wrote_shared_hooks_for(
        worktree.path(),
        AN_ORCHESTRATOR,
        serde_json::json!({ "permissions": { "allow": ["Bash(ls:*)"] } }),
    );

    // When a launch prepares the worktree
    prepare_worktree_for_session_hooks(&tddy_fs::DiskFs, 
        &the_claude_spec(),
        worktree.path(),
        &slots_with_the_hooks_of(worktree.path(), THE_HOME_SESSION),
        &PointerHost::Local,
    )
    .await
    .expect("the worktree is prepared");

    // Then only the permission rule is left
    let kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(legacy_file(worktree.path())).unwrap()).unwrap();
    assert_eq!(
        kept,
        serde_json::json!({ "permissions": { "allow": ["Bash(ls:*)"] } })
    );
}

#[tokio::test]
async fn a_launch_with_no_session_hooks_file_leaves_the_worktree_alone() {
    // Given the daemon's shared hooks, and a launch that renders no hooks of its own
    let worktree = a_checkout();
    the_daemon_wrote_shared_hooks_for(worktree.path(), AN_ORCHESTRATOR, serde_json::json!({}));

    // When it prepares the worktree
    prepare_worktree_for_session_hooks(&tddy_fs::DiskFs, 
        &the_claude_spec(),
        worktree.path(),
        &RenderedSlots::default(),
        &PointerHost::Local,
    )
    .await
    .expect("nothing to prepare");

    // Then nothing was touched
    assert!(legacy_file(worktree.path()).exists());
    assert!(!the_exclude_file_of(worktree.path()).contains("/.tddy/hooks/"));
}

#[tokio::test]
async fn on_a_target_the_shared_hooks_are_released_and_the_hooks_directory_excluded_through_its_shell(
) {
    // Given a target's worktree whose shared settings hold the daemon's hooks
    let worktree = a_checkout();
    the_daemon_wrote_shared_hooks_for(worktree.path(), AN_ORCHESTRATOR, serde_json::json!({}));

    // When a launch prepares it through the target's shell
    prepare_worktree_for_session_hooks(&tddy_fs::DiskFs, 
        &the_claude_spec(),
        worktree.path(),
        &slots_with_the_hooks_of(worktree.path(), THE_HOME_SESSION),
        &on_a_target(),
    )
    .await
    .expect("the worktree is prepared on the target");

    // Then the target's shared file is gone, and its checkout excludes the hooks directory
    assert!(!legacy_file(worktree.path()).exists());
    assert!(the_exclude_file_of(worktree.path())
        .lines()
        .any(|l| l == "/.tddy/hooks/"));
}

#[tokio::test]
async fn on_a_target_with_no_shared_file_only_the_exclude_is_added() {
    // Given a target's worktree that never had shared hooks
    let worktree = a_checkout();

    // When a launch prepares it, twice
    for _ in 0..2 {
        prepare_worktree_for_session_hooks(&tddy_fs::DiskFs, 
            &the_claude_spec(),
            worktree.path(),
            &slots_with_the_hooks_of(worktree.path(), THE_HOME_SESSION),
            &on_a_target(),
        )
        .await
        .expect("the worktree is prepared on the target");
    }

    // Then the exclude is there once, and no shared file was made
    let excludes = the_exclude_file_of(worktree.path());
    assert_eq!(
        excludes.lines().filter(|l| *l == "/.tddy/hooks/").count(),
        1,
        "{excludes:?}"
    );
    assert!(!legacy_file(worktree.path()).exists());
}
