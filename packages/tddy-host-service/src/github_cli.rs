//! The host's own GitHub CLI, asked as an OS user: who it is logged in as, and its token.
//!
//! A project set to **Local gh** takes its GitHub identity from this, instead of from a vault
//! account (docs/ft/daemon/github-identity.md). `gh` keeps its login per OS user
//! (`$HOME/.config/gh/hosts.yml`, or the keyring), so every question goes through
//! [`tddy_daemon_kernel::spawn_as_user`], exactly as the tooling probe does
//! ([`crate::host_tooling`]).
//!
//! Every question names the GitHub **host** (`--hostname`): a person logged in to `github.com` and
//! to an Enterprise host is two logins, and the project's git URL says which one it means.
//!
//! The environment is never a credential: `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` and
//! `GITHUB_ENTERPRISE_TOKEN` are not passed to `gh`, which would otherwise prefer them over the login
//! this exists to read.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::host_tooling::{start_probe, summarise, ProbeOutput, PROBE_TIMEOUT};

/// Which `gh` to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCli {
    program: GhProgram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GhProgram {
    /// Looked up on the spawned child's `PATH` at each call, so a `gh` installed after the daemon
    /// started is found, and one removed is reported as not installed.
    OnSpawnChildPath,
    /// An explicit executable.
    At(PathBuf),
}

/// The GitHub user `gh` is logged in as, for one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhUser {
    pub login: String,
    /// GitHub's immutable user id — the half of the noreply address a renamed login cannot change.
    pub id: u64,
}

/// Why `gh` gave no answer. Each names what the person would do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubCliRefusal {
    /// No `gh` executable on this host.
    NotInstalled,
    /// `gh` is installed and is not logged in to `hostname` as `os_user`.
    NotLoggedIn { hostname: String, os_user: String },
    /// `gh` could not be run, timed out, or answered something this code does not recognise.
    Failed(String),
}

impl std::fmt::Display for GithubCliRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str(
                "the GitHub CLI (gh) is not installed on this host; install it and run `gh auth login`",
            ),
            Self::NotLoggedIn { hostname, os_user } => write!(
                f,
                "gh is not logged in to {hostname} as OS user {os_user}; run `gh auth login --hostname {hostname}` as {os_user}"
            ),
            Self::Failed(reason) => write!(f, "gh could not answer: {reason}"),
        }
    }
}

impl GithubCli {
    /// The `gh` on the `PATH` a spawned child runs with, looked up at each call.
    pub fn on_spawn_child_path() -> Self {
        Self {
            program: GhProgram::OnSpawnChildPath,
        }
    }

    /// The `gh` at `program`.
    pub fn at(program: PathBuf) -> Self {
        Self {
            program: GhProgram::At(program),
        }
    }

    /// Who `gh api user --hostname <hostname>` says it is logged in as, asked as `os_user`.
    pub fn logged_in_user(
        &self,
        os_user: &str,
        hostname: &str,
    ) -> Result<GhUser, GithubCliRefusal> {
        let stdout = self.ask(os_user, hostname, &["api", "user", "--hostname", hostname])?;
        let unrecognised = || {
            GithubCliRefusal::Failed(format!(
                "gh api user answered something this daemon does not recognise: {}",
                summarise(&stdout)
            ))
        };
        let answer: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|_| unrecognised())?;
        match (
            answer.get("login").and_then(serde_json::Value::as_str),
            answer.get("id").and_then(serde_json::Value::as_u64),
        ) {
            (Some(login), Some(id)) if !login.is_empty() => Ok(GhUser {
                login: login.to_string(),
                id,
            }),
            _ => Err(unrecognised()),
        }
    }

    /// What `gh auth token --hostname <hostname>` answers, asked as `os_user`.
    pub fn auth_token(&self, os_user: &str, hostname: &str) -> Result<String, GithubCliRefusal> {
        let stdout = self.ask(
            os_user,
            hostname,
            &["auth", "token", "--hostname", hostname],
        )?;
        let token = stdout.trim();
        if token.is_empty() {
            return Err(GithubCliRefusal::Failed(
                "gh auth token answered with no token".to_string(),
            ));
        }
        Ok(token.to_string())
    }

    /// Run `gh <args>` as `os_user` and return what it wrote to stdout.
    ///
    /// Bounded by [`PROBE_TIMEOUT`]; the child is ended when it overruns. The token variables are
    /// removed by `spawn_as_user`, so `gh` answers from its own login.
    #[cfg(unix)]
    fn ask(
        &self,
        os_user: &str,
        hostname: &str,
        args: &[&str],
    ) -> Result<String, GithubCliRefusal> {
        let program = match &self.program {
            GhProgram::OnSpawnChildPath => {
                tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path("gh")
            }
            GhProgram::At(path) => Path::new(path).is_file().then(|| path.clone()),
        }
        .ok_or(GithubCliRefusal::NotInstalled)?;

        let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
        let output: ProbeOutput = start_probe(os_user, &program, args)
            .collect(deadline)
            .map_err(GithubCliRefusal::Failed)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        match output.status.code() {
            Some(0) => Ok(stdout),
            Some(code) => {
                let said = stderr.to_ascii_lowercase();
                if said.contains("gh auth login")
                    || said.contains("no oauth token found")
                    || said.contains("not logged in")
                {
                    Err(GithubCliRefusal::NotLoggedIn {
                        hostname: hostname.to_string(),
                        os_user: os_user.to_string(),
                    })
                } else {
                    Err(GithubCliRefusal::Failed(format!(
                        "gh {} exited with {code}: {}",
                        args.join(" "),
                        summarise(&stderr)
                    )))
                }
            }
            None => Err(GithubCliRefusal::Failed(format!(
                "gh {} was killed by a signal",
                args.join(" ")
            ))),
        }
    }

    /// `start_output_as_user` is Unix-only, so there is nothing to run here.
    #[cfg(not(unix))]
    fn ask(
        &self,
        _os_user: &str,
        _hostname: &str,
        _args: &[&str],
    ) -> Result<String, GithubCliRefusal> {
        Err(GithubCliRefusal::Failed(
            "asking gh as another OS user is supported on Unix only".to_string(),
        ))
    }
}
