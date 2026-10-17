//! Real-process fixtures for the clone passphrase acceptance suite.
//!
//! Nothing here fakes OpenSSH. A clone is only worth testing against the real `git`, `ssh`, `sshd`
//! and `ssh-agent`: the behaviours this feature depends on — which prompt text ssh emits, when it
//! calls its askpass helper, whether `AddKeysToAgent` fires — are exactly what a hand-written fake
//! would encode as an assumption. Only the human is replaced, by [`operator::ScriptedOperator`].

pub mod operator;
pub mod rig;
pub mod ssh_agent;
pub mod sshd;

use std::path::PathBuf;
use std::process::Command;

/// The OS user this test process runs as — the user a same-user clone runs as, so no privilege drop
/// is involved and the suite needs no root.
pub fn current_os_user() -> String {
    let out = Command::new("id")
        .arg("-un")
        .output()
        .expect("`id -un` runs");
    assert!(out.status.success(), "`id -un` failed");
    String::from_utf8(out.stdout)
        .expect("a user name is UTF-8")
        .trim()
        .to_string()
}

/// A binary built into the workspace's `target/debug`, by name.
///
/// The suite runs in a package that does not own `tddy-askpass`, so cargo sets no `CARGO_BIN_EXE_`
/// for it and does not rebuild it on this package's behalf — the same situation the sandbox-runner
/// suites are in, and `./test` builds it explicitly for the same reason. A missing binary is a
/// stated failure, not a skip: a suite that passed without the helper would not be testing it.
pub fn built_binary(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name);
    assert!(
        path.is_file(),
        "{} is not built. `./test` builds it; a bare `cargo test -p tddy-agent-launch` does not — \
         run `cargo build -p {name}` first",
        path.display()
    );
    path
}
