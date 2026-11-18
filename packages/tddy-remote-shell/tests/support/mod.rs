//! Stand-in `ssh` executables: shell scripts on a temp path that take the place of the OpenSSH
//! client, so no suite opens a connection.
#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use tddy_remote_shell::SshShell;

/// An executable `ssh` stand-in and the directory it lives in.
pub struct StandInSsh {
    dir: tempfile::TempDir,
}

/// An ssh whose target is this host: it records the arguments of each call on a line of its own
/// and runs its last argument, the remote command, with `sh -c`.
pub fn an_ssh_that_runs_commands_here() -> StandInSsh {
    StandInSsh::running(
        "printf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/calls\"\n\
         for last; do :; done\n\
         exec sh -c \"$last\"\n",
    )
}

/// An ssh as OpenSSH behaves with a `ProxyCommand`: it records its arguments, runs the
/// ProxyCommand's program once (its input and output are nothing: the stand-in's workspace is
/// this host), then runs its last argument, the remote command, with `sh -c` over plain pipes.
pub fn an_ssh_that_connects_through_its_proxy_and_runs_commands_here() -> StandInSsh {
    StandInSsh::running(
        "printf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/calls\"\n\
         proxy=\n\
         for argument; do\n\
           case \"$argument\" in ProxyCommand=*) proxy=\"${argument#ProxyCommand=}\";; esac\n\
           last=\"$argument\"\n\
         done\n\
         if [ -n \"$proxy\" ]; then sh -c \"$proxy\" </dev/null >/dev/null 2>&1; fi\n\
         exec sh -c \"$last\"\n",
    )
}

/// An ssh that connects to nothing and never returns.
pub fn an_ssh_that_never_answers() -> StandInSsh {
    StandInSsh::running("exec sleep 30\n")
}

/// An ssh that fails as OpenSSH does when it cannot reach its target: `complaint` on stderr and
/// exit status 255, with the remote command never run.
pub fn an_ssh_that_cannot_reach_its_target(complaint: &str) -> StandInSsh {
    StandInSsh::running(&format!("echo '{complaint}' >&2\nexit 255\n"))
}

impl StandInSsh {
    fn running(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("a temp dir for the stand-in ssh");
        let path = dir.path().join("ssh");
        // The warm-up guard is the script's first line and it is run once, guarded, here: a
        // freshly written executable pays a first-execution scan on this OS (~2.4 s measured
        // on macOS, against ~20 ms afterwards), and a stand-in timed from its first real exec
        // would spend that scan inside whatever window its test measures. Guarded, the
        // warm-up records no call and runs nothing.
        std::fs::write(
            &path,
            format!("#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n{body}"),
        )
        .expect("the stand-in ssh is written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("the stand-in ssh is made executable");
        let warmed = std::process::Command::new(&path)
            .env("TDDY_FIXTURE_WARMUP", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("the freshly written stand-in warms up");
        assert!(
            warmed.status.success(),
            "the warm-up of the stand-in ssh exited {}",
            warmed.status
        );
        Self { dir }
    }

    /// Where the stand-in is.
    pub fn path(&self) -> PathBuf {
        self.dir.path().join("ssh")
    }

    /// The shell on `alias` reached through this stand-in.
    pub fn shell_on(&self, alias: &str) -> SshShell {
        SshShell::through(self.path(), alias)
    }

    /// The arguments of every call so far, one call per entry, joined by spaces.
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// A directory on the "target" a test may write under.
    pub fn scratch_dir(&self) -> PathBuf {
        self.dir.path().join("target-files")
    }
}
