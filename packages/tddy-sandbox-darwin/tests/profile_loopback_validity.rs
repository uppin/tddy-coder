//! Verify loopback network policy renders a valid Seatbelt profile.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::process::Command;

use tddy_sandbox::{NetworkSpec, SandboxBuilder};
use tddy_sandbox_darwin::render_plan;

/// **loopback_network_profile_is_accepted_by_sandbox_exec**: pre-declared loopback ports
/// produce a profile under which a command actually *runs to completion*.
///
/// This asserts `status.success()` and the expected stdout — not merely "exit code is not
/// 6". The weaker `code() != Some(6)` check gave false confidence during the SIGABRT
/// investigation: a child that aborts in `dyld` is terminated by a signal, so
/// `status.code()` is `None` (never `Some(6)`) and the old assertion passed while `echo`
/// never ran. See packages/tddy-sandbox-darwin/docs/troubleshooting.md.
#[test]
fn loopback_network_profile_is_accepted_by_sandbox_exec() {
    // Given
    let project_root = std::path::PathBuf::from("/tmp/tddy-sandbox-loopback-profile");
    let scratch = project_root.join(".work");
    let egress = project_root.join("out");
    let profile_path = project_root.join("profile.sb");
    let mut env = BTreeMap::new();
    env.insert("PATH".into(), "/usr/bin:/bin".into());

    let plan = SandboxBuilder::new(
        &project_root,
        &scratch,
        &egress,
        vec!["/bin/echo".into(), "hi".into()],
    )
    .profile_path(&profile_path)
    .reads(tddy_sandbox::system_baseline_reads())
    .policy(tddy_sandbox::claude_policy())
    .network(NetworkSpec {
        loopback_allow_ports: vec![55900, 55901],
        allow_oauth_inbound: false,
    })
    .env_map(env)
    .build()
    .expect("plan must build");

    let profile = render_plan(&plan).expect("render profile");
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
