//! Lower-level VFS rule tests for remote sandbox (daemon host side).

#[test]
fn vfs_accepts_safe_relative_path() {
    tddy_daemon::remote_sandbox::vfs::ensure_relative_under_root("session-a/work/readme.md")
        .expect("safe relative path must be accepted");
}

#[test]
fn vfs_rejects_dotdot() {
    assert_eq!(
        tddy_daemon::remote_sandbox::vfs::ensure_relative_under_root("x/../../../secret"),
        Err("path escapes sandbox root (..)")
    );
}
