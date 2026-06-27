//! Verify loopback network policy renders a valid Seatbelt profile.

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;

use tddy_sandbox::SandboxSpec;
use tddy_sandbox_darwin::render_profile;

/// **loopback_network_profile_is_accepted_by_sandbox_exec**: pre-declared loopback ports
/// produce a profile under which a command actually *runs to completion*.
///
/// This asserts `status.success()` and the expected stdout — not merely "exit code is not
/// 6". The weaker `code() != Some(6)` check gave false confidence during the SIGABRT
/// investigation: a child that aborts in `dyld` is terminated by a signal, so
/// `status.code()` is `None` (never `Some(6)`) and the old assertion passed while `echo`
/// never ran. See docs/dev/1-WIP/2026-06-27-darwin-sandbox-seatbelt-investigation.md.
#[test]
fn loopback_network_profile_is_accepted_by_sandbox_exec() {
    // Given
    let spec = SandboxSpec {
        project_root: PathBuf::from("/tmp/tddy-sandbox-loopback-profile"),
        scratch_dir: PathBuf::from("/tmp/tddy-sandbox-loopback-profile/.work"),
        egress_dir: PathBuf::from("/tmp/tddy-sandbox-loopback-profile/out"),
        allow_read_paths: vec![PathBuf::from("/usr/bin")],
        command: vec!["/bin/echo".into(), "hi".into()],
        env: Default::default(),
        profile_path: PathBuf::from("/tmp/tddy-sandbox-loopback-profile/profile.sb"),
        loopback_allow_ports: vec![55900, 55901],
        ipc_socket: None,
    };
    let profile = render_profile(&spec).expect("render profile");
    let profile_path = spec.profile_path.clone();
    std::fs::create_dir_all(profile_path.parent().expect("profile parent")).expect("mkdir");
    std::fs::write(&profile_path, profile).expect("write profile");

    // When
    let output = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile_path)
        .arg("/bin/echo")
        .arg("hi")
        .output()
        .expect("sandbox-exec");

    // Then
    assert!(
        output.status.success(),
        "command must run to completion under the loopback profile \
         (status={:?}, exit 6 = invalid SBPL, signal termination = runtime abort)\n\
         stderr={}\nprofile path={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        profile_path.display()
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "hi",
        "sandboxed echo must produce its output"
    );
}
