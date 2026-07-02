//! Acceptance: `tddy-sandbox-cgroups::spawn_plan` must pipe a jailed process's stdin/stdout when
//! `--stdio` is present in its command, mirroring `tddy-sandbox-darwin::spawn_plan`'s
//! `stdio_mode` branch — see docs/dev/TODO.md ("Linux (`tddy-sandbox-cgroups`) jail-spawn stdio
//! piping"). Without this, `tddy-daemon`'s stdio-based session control channel (Item A of the
//! `finish-stdio-ipc-migration` changeset) only works on macOS.
//!
//! Requires a host that permits unprivileged user namespaces; self-skips otherwise, same pattern
//! as `jail_smoke.rs`.
#![cfg(target_os = "linux")]

use std::io::Write;

use tddy_sandbox::{EnvSpec, NetworkSpec, PolicySpec, ResourceLimits, SandboxPlan, SandboxSpec};
use tddy_sandbox_cgroups::{spawn_plan, unprivileged_userns_available};

/// **spawn_plan_pipes_stdin_and_stdout_when_stdio_flag_is_present**: a runner command containing
/// the literal `--stdio` token gets its stdin/stdout piped back to the caller — instead of
/// stdout being redirected to the egress log, the non-`--stdio` default — so
/// `SandboxHandle::take_stdio()` returns `Some` and bytes written to the piped stdin actually
/// reach the jailed process.
#[test]
fn spawn_plan_pipes_stdin_and_stdout_when_stdio_flag_is_present() {
    if !unprivileged_userns_available() {
        eprintln!("SKIP: host forbids unprivileged user namespaces (cannot create the jail here)");
        return;
    }

    // Given a plan whose command requests the stdio transport (the trailing "--stdio" is a
    // positional parameter to the `sh -c` script below, ignored by the script itself — only its
    // literal presence in `spec.command` matters to `spawn_plan`'s stdio-mode detection)
    let tmp = tempfile::tempdir().unwrap();
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(&project_root).unwrap();
    let spec = SandboxSpec {
        project_root: project_root.clone(),
        scratch_dir: tmp.path().join("scratch"),
        egress_dir: tmp.path().join("egress"),
        allow_read_paths: vec![],
        command: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
            "--stdio".to_string(),
        ],
        env: Default::default(),
        profile_path: tmp.path().join("profile.sb"),
        loopback_allow_ports: vec![],
        ipc_socket: None,
        cwd: None,
    };
    let plan = SandboxPlan {
        spec,
        reads: vec![],
        mounts: vec![],
        copies: vec![],
        symlinks: vec![],
        env: EnvSpec::default(),
        policy: PolicySpec::default(),
        network: NetworkSpec::default(),
        limits: ResourceLimits::default(),
        stdin: None,
    };

    // When
    let mut handle = spawn_plan(plan).expect("spawn cgroups jail with --stdio");

    // Then
    let (mut stdin, mut stdout) = handle
        .take_stdio()
        .expect("stdio must be piped when --stdio is in the command");
    stdin
        .write_all(b"stdio-piping-round-trip\n")
        .expect("write to piped stdin");
    drop(stdin);

    use std::io::Read;
    let mut echoed = String::new();
    stdout
        .read_to_string(&mut echoed)
        .expect("read piped stdout");
    assert_eq!(echoed, "stdio-piping-round-trip\n");

    handle.child_mut().wait().ok();
}
