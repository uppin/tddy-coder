//! Stand-in `coder` executables: shell scripts on a temp path that take the place of the Coder
//! CLI, so no suite reaches a deployment.
#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use tddy_credentials::SecretString;
use tddy_remote_shell::{CoderCredentials, CoderShell, CoderWorkspaceRef};

/// The deployment every stand-in is reached as.
pub const DEPLOYMENT_URL: &str = "https://coder.example.com";
/// The session token every stand-in is given.
pub const SESSION_TOKEN: &str = "adas-coder-session-token";

/// Records the arguments of each call on a line of `calls`, and the two variables the CLI reads
/// its deployment and token from on a line of `environment`.
const RECORD_THE_CALL: &str = "here=\"$(dirname \"$0\")\"\n\
     printf '%s\\n' \"$*\" >> \"$here/calls\"\n\
     printf 'CODER_URL=%s CODER_SESSION_TOKEN=%s\\n' \"${CODER_URL-}\" \"${CODER_SESSION_TOKEN-}\" \
     >> \"$here/environment\"\n";

/// An executable `coder` stand-in and the directory it lives in.
pub struct StandInCoder {
    dir: tempfile::TempDir,
}

/// A coder whose every workspace is this host: it records each call, then hands what follows
/// `--` to a shell as one command line — which is what a workspace's login shell is given.
pub fn a_coder_whose_workspaces_run_commands_here() -> StandInCoder {
    StandInCoder::running(&format!(
        "{RECORD_THE_CALL}\
         while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
         shift\n\
         exec sh -c \"$*\"\n"
    ))
}

/// A coder whose workspaces run commands in a pty, as the real `coder ssh` does: the
/// workspace's stderr is merged into its stdout, so nothing the command writes to stderr
/// arrives on a separate stream.
pub fn a_coder_whose_workspaces_merge_stderr_into_stdout() -> StandInCoder {
    StandInCoder::running(&format!(
        "{RECORD_THE_CALL}\
         while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
         shift\n\
         exec sh -c \"$*\" 2>&1\n"
    ))
}

/// A coder whose workspaces run commands in a pty that never ends their input, as the real
/// `coder ssh` does: what is written to the CLI's stdin reaches the command, but closing it does
/// not — the pty turns no end of input into an EOF — and the workspace's stderr is merged into its
/// stdout. The command's stdin is a FIFO the command itself holds open for writing, so a read past
/// the bytes it was sent blocks for good, as `cat` does on the workspace. (The CLI's stdin is
/// kept as fd 4 for the forwarder: an asynchronous list's own stdin is `/dev/null`.)
pub fn a_coder_whose_workspaces_never_see_the_end_of_their_input() -> StandInCoder {
    StandInCoder::running(&format!(
        "{RECORD_THE_CALL}\
         while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
         shift\n\
         input=\"$here/input.$$\"\n\
         mkfifo \"$input\"\n\
         exec 3<>\"$input\" 4<&0\n\
         cat <&4 >&3 &\n\
         forwarder=$!\n\
         sh -c \"$*\" <&3 2>&1\n\
         status=$?\n\
         kill \"$forwarder\" 2>/dev/null\n\
         rm -f \"$input\"\n\
         exit \"$status\"\n"
    ))
}

/// A coder whose workspaces run commands in a pty that ends every line the command writes with a
/// carriage return (`onlcr`), as the real `coder ssh` does, until the command turns the pty's
/// output processing off with `stty -opost`. The workspace's `stty` is a stand-in that only notes
/// that; text alone is translated — a line is read by the shell, so a NUL would not survive it —
/// and the CLI exits with the translation's status, not the command's.
pub fn a_coder_whose_workspaces_end_their_lines_with_carriage_returns() -> StandInCoder {
    StandInCoder::running(&format!(
        "{RECORD_THE_CALL}\
         while [ \"$#\" -gt 0 ] && [ \"$1\" != \"--\" ]; do shift; done\n\
         shift\n\
         mkdir -p \"$here/workspace-bin\"\n\
         printf '#!/bin/sh\\ncase \" $* \" in *\" -opost \"*) : > \"%s/opost-off\";; esac\\n' \"$here\" \
         > \"$here/workspace-bin/stty\"\n\
         chmod +x \"$here/workspace-bin/stty\"\n\
         rm -f \"$here/opost-off\"\n\
         PATH=\"$here/workspace-bin:$PATH\" sh -c \"$*\" 2>&1 | \
         while IFS= read -r line || {{ printf '%s' \"$line\"; false; }}; do\n\
           if [ -f \"$here/opost-off\" ]; then ending='\\n'; else ending='\\r\\n'; fi\n\
           printf \"%s$ending\" \"$line\"\n\
         done\n"
    ))
}

/// A coder that refuses before anything runs on a workspace, as the CLI does for a workspace it
/// will not connect to: `complaint` on stderr and exit status 1.
pub fn a_coder_that_refuses_to_connect(complaint: &str) -> StandInCoder {
    StandInCoder::running(&format!(
        "{RECORD_THE_CALL}echo '{complaint}' >&2\nexit 1\n"
    ))
}

impl StandInCoder {
    fn running(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("a temp dir for the stand-in coder");
        let path = dir.path().join("coder");
        // The warm-up guard is the script's first line and it is run once, guarded, here: a
        // freshly written executable pays a first-execution scan on this OS (~2.4 s measured
        // on macOS, against ~20 ms afterwards), and a stand-in timed from its first real exec
        // would spend that scan inside whatever window its test measures. Guarded, the
        // warm-up records no call and runs nothing.
        std::fs::write(
            &path,
            format!("#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n{body}"),
        )
        .expect("the stand-in coder is written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("the stand-in coder is made executable");
        let warmed = std::process::Command::new(&path)
            .env("TDDY_FIXTURE_WARMUP", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("the freshly written stand-in warms up");
        assert!(
            warmed.status.success(),
            "the warm-up of the stand-in coder exited {}",
            warmed.status
        );
        Self { dir }
    }

    /// Where the stand-in is.
    pub fn path(&self) -> PathBuf {
        self.dir.path().join("coder")
    }

    /// The shell on `workspace` reached through this stand-in, as [`DEPLOYMENT_URL`]'s user with
    /// [`SESSION_TOKEN`].
    pub fn shell_on(&self, workspace: CoderWorkspaceRef) -> CoderShell {
        CoderShell::through(self.path(), the_credentials(), workspace)
    }

    /// The arguments of every call so far, one call per entry, joined by spaces.
    pub fn calls(&self) -> Vec<String> {
        self.lines_of("calls")
    }

    /// What every call so far was given as its deployment and token, one call per entry:
    /// `CODER_URL=<url> CODER_SESSION_TOKEN=<token>`.
    pub fn environments(&self) -> Vec<String> {
        self.lines_of("environment")
    }

    /// A directory on the "workspace" a test may write under.
    pub fn scratch_dir(&self) -> PathBuf {
        self.dir.path().join("workspace-files")
    }

    fn lines_of(&self, record: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join(record))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// [`DEPLOYMENT_URL`] with [`SESSION_TOKEN`].
pub fn the_credentials() -> CoderCredentials {
    CoderCredentials::new(DEPLOYMENT_URL, SecretString::new(SESSION_TOKEN))
}
