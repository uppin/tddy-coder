//! The stand-in **Coder workspace** the Coder-target suites (`coder_agent_launch_acceptance`,
//! `target_shell_tab_acceptance`) reach: a directory standing for the workspace's filesystem,
//! holding one session's worktree, the stand-in `coder` CLI that reaches it and the stand-in OpenSSH
//! `ssh` a streamed command goes through. Shared, not copied (todo
//! `2026-11-16-remote-shell-test-fixtures-are-duplicated`).
//!
//! The workspace's login shell has a `HOME` and a `PATH` of its own: `-lc <command>` runs a
//! command there, as an agent's start does; `-l` — a person's login shell, as a shell tab runs it —
//! records the directory it started in ([`AWorkspace::login_shell_started_in`]) and becomes an
//! interactive `/bin/sh` on its terminal.

#![allow(dead_code)]

use std::path::PathBuf;

use tddy_testing_commons::remote_target::write_executable;

/// A Coder workspace the daemon reaches: a directory standing for its filesystem, holding the
/// session's worktree, and the stand-in `coder` that reaches it.
pub struct AWorkspace {
    pub root: PathBuf,
    session_id: String,
    _dir: tempfile::TempDir,
}

/// One call of the stand-in `coder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ACoderCall {
    /// The directory the CLI was run in.
    pub cwd: PathBuf,
    /// The deployment it was given in `CODER_URL`.
    pub deployment: String,
    /// The token it was given in `CODER_SESSION_TOKEN`.
    pub token: String,
    /// Its arguments, joined by spaces.
    pub args: String,
}

/// A running workspace holding the worktree of `session_id`: the stand-in records its working
/// directory, the deployment and token it was given and its arguments, then hands what follows `--`
/// to a shell as one command line — which is what a workspace's login shell is given — and becomes
/// it. Like coder v2.33.5, it refuses `-t`, a flag that CLI does not have. As `coder ssh --stdio`,
/// the stand-in `ssh`'s `ProxyCommand`, it is the pipe to the workspace and carries nothing: that
/// `ssh` runs its command here itself.
pub fn a_running_workspace(session_id: &str) -> AWorkspace {
    a_workspace_answering_one_shots_after(session_id, "")
}

/// [`a_running_workspace`], running `delay` (a shell snippet) before every command it hands on.
pub fn a_workspace_answering_one_shots_after(session_id: &str, delay: &str) -> AWorkspace {
    let dir = tempfile::tempdir().expect("a directory standing for the workspace");
    let root = std::fs::canonicalize(dir.path()).expect("the workspace's directory resolves");
    std::fs::create_dir_all(root.join("bin")).expect("a directory for the stand-in coder");
    write_executable(
        &root.join("bin/coder"),
        &format!(
            r#"#!/bin/sh
printf '%s\t%s\t%s\t%s\n' "$(/bin/pwd -P)" "${{CODER_URL-}}" "${{CODER_SESSION_TOKEN-}}" \
  "$(printf '%s' "$*" | /usr/bin/tr '\n\t' '  ')" >> "$(dirname "$0")/calls"
for argument in "$@"; do
  [ "$argument" = "--" ] && break
  if [ "$argument" = "-t" ]; then
    echo "unknown shorthand flag: 't' in -t" >&2
    exit 1
  fi
done
[ "$2" = "--stdio" ] && exit 0
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do shift; done
shift
{delay}SHELL="$(dirname "$0")/login-shell"
export SHELL
exec /bin/sh -c "$*"
"#
        ),
    );
    write_executable(
        &root.join("bin/ssh"),
        r#"#!/bin/sh
proxy=
for argument; do
  case "$argument" in ProxyCommand=*) proxy="${argument#ProxyCommand=}";; esac
  last="$argument"
done
[ -n "$proxy" ] && /bin/sh -c "$proxy" </dev/null >/dev/null 2>&1
SHELL="$(dirname "$0")/login-shell"
export SHELL
exec /bin/sh -c "$last"
"#,
    );
    let workspace = AWorkspace {
        root,
        session_id: session_id.to_string(),
        _dir: dir,
    };
    for dir in [
        workspace.worktree(),
        workspace.home(),
        workspace.agents_dir(),
    ] {
        std::fs::create_dir_all(dir).expect("a directory of the workspace");
    }
    // The workspace's login shell: a `HOME` and a `PATH` of its own, which holds its agents.
    write_executable(
        &workspace.root.join("bin/login-shell"),
        &format!(
            "#!/bin/sh\nHOME='{}'\nPATH='{}:/usr/bin:/bin'\nexport HOME PATH\n\
             if [ \"$1\" = -l ]; then /bin/pwd -P > '{}'; exec /bin/sh; fi\n\
             [ \"$1\" = -lc ] || exit 64\nexec /bin/sh -c \"$2\"\n",
            workspace.home().display(),
            workspace.agents_dir().display(),
            workspace.login_shell_started_in().display()
        ),
    );
    workspace
}

impl AWorkspace {
    /// The stand-in `coder` that reaches this workspace.
    pub fn coder(&self) -> PathBuf {
        self.root.join("bin/coder")
    }

    /// The stand-in OpenSSH client a streamed command reaches this workspace through.
    pub fn ssh(&self) -> PathBuf {
        self.root.join("bin/ssh")
    }

    /// The `HOME` of the workspace's login.
    pub fn home(&self) -> PathBuf {
        self.root.join("home/coder")
    }

    /// Where the workspace's login finds its agents: the first directory of its `PATH`.
    pub fn agents_dir(&self) -> PathBuf {
        self.root.join("opt/agents")
    }

    /// The session's worktree in the workspace.
    pub fn worktree(&self) -> PathBuf {
        self.root
            .join("home/coder/repo/.worktrees")
            .join(&self.session_id)
    }

    /// Where the workspace's login shell, run as a person's login shell (`-l`), records the
    /// directory it started in.
    pub fn login_shell_started_in(&self) -> PathBuf {
        self.root.join("bin/login-shell-started-in")
    }

    /// Every call of the stand-in so far.
    pub fn calls(&self) -> Vec<ACoderCall> {
        std::fs::read_to_string(self.root.join("bin/calls"))
            .unwrap_or_default()
            .lines()
            .map(|line| line.splitn(4, '\t').collect::<Vec<_>>())
            .filter(|fields| fields.len() == 4)
            .map(|fields| ACoderCall {
                cwd: PathBuf::from(fields[0]),
                deployment: fields[1].to_string(),
                token: fields[2].to_string(),
                args: fields[3].to_string(),
            })
            .collect()
    }

    /// Every call of the stand-in so far whose arguments hold `needle`.
    pub fn calls_carrying(&self, needle: &str) -> Vec<ACoderCall> {
        self.calls()
            .into_iter()
            .filter(|call| call.args.contains(needle))
            .collect()
    }
}
