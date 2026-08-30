//! Unit: the workspace tool jail's layout, its sandbox plan, and the platform gate in front of it.
//!
//! Changeset: `docs/dev/1-WIP/2026-08-30-workspace-tool-sandbox.md`.
//!
//! These pin what the jail is *declared* to be — where its artifacts live, what of the host is
//! inside it, and what runs in it — without spawning one. What the kernel then enforces is proven
//! by the two real-jail suites named in `workspace_tool_sandbox_acceptance.rs`.

use std::path::{Path, PathBuf};

use tddy_daemon::workspace_tool_sandbox::{
    build_workspace_tool_plan, workspace_sandbox_platform_support, WorkspaceSandboxLayout,
    WorkspaceToolPlanRequest,
};
use tddy_sandbox::SandboxPlan;

const SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a42";

/// A session directory and a worktree beside it, as a workspace start leaves them.
struct SessionOnDisk {
    session_dir: PathBuf,
    worktree: PathBuf,
    _tmp: tempfile::TempDir,
}

fn a_session_on_disk() -> SessionOnDisk {
    let tmp = tempfile::tempdir().expect("tempdir");
    let session_dir = tmp.path().join("sessions").join(SESSION_ID);
    let worktree = session_dir.join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    SessionOnDisk {
        session_dir,
        worktree,
        _tmp: tmp,
    }
}

fn a_workspace_tool_plan(session: &SessionOnDisk) -> SandboxPlan {
    build_workspace_tool_plan(WorkspaceToolPlanRequest {
        layout: WorkspaceSandboxLayout::under_session_dir(&session.session_dir),
        worktree_path: session.worktree.clone(),
        session_id: SESSION_ID.to_string(),
        runner_path: "/opt/tddy/bin/tddy-sandbox-runner".to_string(),
        tddy_tools_path: "/opt/tddy/bin/tddy-tools".to_string(),
        cgroup: Default::default(),
    })
    .expect("a workspace tool plan must build")
}

/// True when `path` is inside `base` — the jail's writable tree is defined by containment, not by
/// exact equality, so the assertions below say so directly.
fn is_under(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// The jail's artifacts live with the session that owns them, so deleting the session directory
/// takes the jail's scratch, profile and egress with it rather than leaving them on the host.
#[test]
fn the_jail_layout_is_rooted_at_the_session_directorys_sandbox_folder() {
    // Given
    let session = a_session_on_disk();

    // When
    let layout = WorkspaceSandboxLayout::under_session_dir(&session.session_dir);

    // Then
    assert_eq!(layout.sandbox_root, session.session_dir.join("sandbox"));
    for path in [
        &layout.scratch_dir,
        &layout.context_dir,
        &layout.egress_dir,
        &layout.ready_marker,
        &layout.profile_path,
        &layout.tool_ipc_socket,
    ] {
        assert!(
            is_under(path, &layout.sandbox_root),
            "{} must live under the session's sandbox root {}",
            path.display(),
            layout.sandbox_root.display()
        );
    }
}

/// Two sessions on one host each get their own jail tree — a shared one would let a tool in one
/// session's jail read the scratch, and the egress, of another's.
#[test]
fn two_sessions_on_the_same_host_get_separate_jail_trees() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let first = tmp.path().join("sessions").join("session-a");
    let second = tmp.path().join("sessions").join("session-b");

    // When
    let a = WorkspaceSandboxLayout::under_session_dir(&first);
    let b = WorkspaceSandboxLayout::under_session_dir(&second);

    // Then
    assert_ne!(a.sandbox_root, b.sandbox_root);
    assert!(!is_under(&a.sandbox_root, &b.sandbox_root));
    assert!(!is_under(&b.sandbox_root, &a.sandbox_root));
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// The point of the jail: the session's checkout is inside it, and writable, because the tools it
/// serves are the mutating ones.
#[test]
fn the_workspace_tool_plan_mounts_the_sessions_worktree_read_write() {
    // Given
    let session = a_session_on_disk();

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    let worktree_mount = plan
        .mounts
        .iter()
        .find(|m| m.host == session.worktree)
        .expect("the session's worktree must be mounted into the jail");
    assert!(
        worktree_mount.writable,
        "a read-only worktree would refuse the Write and Shell calls this jail exists to serve"
    );
}

/// Mounted at its own host path, so a path the daemon resolved outside the jail names the same
/// file inside it — a remapped root would make every tool argument mean two different things.
#[test]
fn the_worktree_is_mounted_at_the_same_path_inside_the_jail_as_on_the_host() {
    // Given
    let session = a_session_on_disk();

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    let worktree_mount = plan
        .mounts
        .iter()
        .find(|m| m.host == session.worktree)
        .expect("the session's worktree must be mounted into the jail");
    assert_eq!(worktree_mount.jail, None);
}

/// The boundary the feature sells: the checkout, and nothing else of the host filesystem. Any
/// second host directory inside the jail is a hole in it, so the mount list is asserted exactly
/// rather than merely searched.
#[test]
fn the_workspace_tool_plan_mounts_nothing_of_the_host_but_the_worktree() {
    // Given
    let session = a_session_on_disk();

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    let mounted: Vec<PathBuf> = plan.mounts.iter().map(|m| m.host.clone()).collect();
    assert_eq!(mounted, vec![session.worktree.clone()]);
}

/// The jail's writable tree is the session's own sandbox root. A scratch or egress directory
/// elsewhere on the host would be a writable path outside the boundary.
#[test]
fn the_jails_writable_tree_is_the_sessions_own_sandbox_root() {
    // Given
    let session = a_session_on_disk();
    let sandbox_root = session.session_dir.join("sandbox");

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    assert!(is_under(&plan.spec.project_root, &sandbox_root));
    assert!(is_under(&plan.spec.scratch_dir, &sandbox_root));
    assert!(is_under(&plan.spec.egress_dir, &sandbox_root));
}

/// The jail is driven over its own piped stdio, the transport `bridge_sandbox_stdio` wraps — the
/// host has no other way to hand it a tool call.
#[test]
fn the_workspace_tool_plan_runs_the_sandbox_runner_in_stdio_mode() {
    // Given
    let session = a_session_on_disk();

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    assert_eq!(
        plan.spec.command.first().map(String::as_str),
        Some("/opt/tddy/bin/tddy-sandbox-runner")
    );
    assert!(
        plan.spec.command.iter().any(|arg| arg == "--stdio"),
        "the workspace jail is driven over stdio, got argv: {:?}",
        plan.spec.command
    );
}

/// The runner runs the session's tools, so it has to know which session it is answering for —
/// every activity row and tool-call record the host writes is keyed by that id.
#[test]
fn the_workspace_tool_plan_names_the_session_the_jail_serves() {
    // Given
    let session = a_session_on_disk();

    // When
    let plan = a_workspace_tool_plan(&session);

    // Then
    let session_id_arg = plan
        .spec
        .command
        .windows(2)
        .find(|pair| pair[0] == "--session-id")
        .map(|pair| pair[1].clone());
    assert_eq!(session_id_arg.as_deref(), Some(SESSION_ID));
}

// ---------------------------------------------------------------------------
// The platform gate
// ---------------------------------------------------------------------------

/// Both backends that exist — Seatbelt and cgroups+namespaces — can hold a workspace jail, so a
/// sandboxed workspace start is servable on either.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_host_with_a_sandbox_backend_admits_a_workspace_jail() {
    // Given a host running one of the two supported backends

    // When
    let support = workspace_sandbox_platform_support();

    // Then
    assert!(
        support.is_ok(),
        "this platform has a sandbox backend, got: {support:?}"
    );
}

/// Anywhere else the answer is a refusal, never a quiet fallback to running the tools on the bare
/// host: a session that came up unconfined is indistinguishable from the one that was asked for.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn a_host_with_no_sandbox_backend_refuses_a_workspace_jail_rather_than_running_on_the_host() {
    // Given a host with no sandbox backend

    // When
    let support = workspace_sandbox_platform_support();

    // Then
    assert!(matches!(
        support,
        Err(tddy_sandbox::SandboxError::Unsupported { .. })
    ));
}
