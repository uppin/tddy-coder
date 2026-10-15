//! A `/bin/sh` stand-in for the GitHub CLI, for tests of code that asks `gh` who it is logged in
//! as and for its token.
//!
//! The stub answers **only** the exact argv a real `gh` logged in to one host would:
//!
//! | argv | logged in to that host | anything else |
//! |---|---|---|
//! | `api user --hostname <host>` | `{"login":…,"id":…}`, exit 0 | gh's "To get started with GitHub CLI" text on stderr, exit 4 |
//! | `auth token --hostname <host>` | the token, exit 0 | `no oauth token found for <host>` on stderr, exit 1 |
//! | `auth status --hostname <host>` | `✓ Logged in to <host> account <login> (keyring)` on stderr, exit 0 | gh's "not logged into any GitHub hosts" text, exit 1 |
//!
//! So a caller that asks about the wrong host, or uses another subcommand, gets a logged-out answer
//! instead of a silently correct one. The wording mirrors gh 2.x; it is not a stable API in gh
//! either, which is exactly why callers must classify unknown output as a failure.
//!
//! [`GhCliStub::recording_env_to`] records the environment of the **last** invocation with one
//! `mv -f`, so a reader never sees a torn record (see [`crate::stub_scripts`]).

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

/// Builder for a `gh` stub. Nothing is written until [`GhCliStub::build`].
#[derive(Debug)]
pub struct GhCliStub {
    path: PathBuf,
    hostname: String,
    login: Option<(String, u64)>,
    token: String,
    env_record: Option<PathBuf>,
}

/// A `gh` stub at `dir/gh`, logged in to nothing until [`GhCliStub::logged_in_as`].
pub fn a_gh_cli(dir: &Path) -> GhCliStub {
    GhCliStub {
        path: dir.join("gh"),
        hostname: "github.com".to_string(),
        login: None,
        token: "gho_token_from_the_hosts_gh".to_string(),
        env_record: None,
    }
}

impl GhCliStub {
    /// Logged in as `login` (GitHub user id `id`) to the stub's host.
    pub fn logged_in_as(mut self, login: &str, id: u64) -> Self {
        self.login = Some((login.to_string(), id));
        self
    }

    /// The one host the stub is logged in to. Defaults to `github.com`.
    pub fn at_host(mut self, hostname: &str) -> Self {
        self.hostname = hostname.to_string();
        self
    }

    /// The token `auth token` answers with. Defaults to `gho_token_from_the_hosts_gh`.
    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.to_string();
        self
    }

    /// Record the environment of the last invocation to `path`, as `env` prints it.
    pub fn recording_env_to(mut self, path: &Path) -> Self {
        self.env_record = Some(path.to_path_buf());
        self
    }

    /// Write the stub, executable, and return its path.
    pub fn build(self) -> PathBuf {
        let host = &self.hostname;
        let mut body = String::from("#!/bin/sh\n");
        if let Some(record) = &self.env_record {
            let record = record.display();
            body.push_str(&format!(
                "env > \"{record}.tmp.$$\" && mv -f \"{record}.tmp.$$\" \"{record}\"\n"
            ));
        }
        if let Some((login, id)) = &self.login {
            let token = &self.token;
            body.push_str(&format!(
                r#"case "$*" in
  "api user --hostname {host}")
    printf '%s\n' '{{"login":"{login}","id":{id},"type":"User"}}'
    exit 0 ;;
  "auth token --hostname {host}")
    printf '%s\n' '{token}'
    exit 0 ;;
  "auth status --hostname {host}")
    printf '%s\n  %s\n' '{host}' '✓ Logged in to {host} account {login} (keyring)' >&2
    exit 0 ;;
esac
"#
            ));
        }
        body.push_str(
            r#"case "$1 $2" in
  "api user")
    printf '%s\n' 'To get started with GitHub CLI, please run:  gh auth login' >&2
    exit 4 ;;
  "auth token")
    printf 'no oauth token found for %s\n' "$4" >&2
    exit 1 ;;
esac
printf '%s\n' 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2
exit 1
"#,
        );
        std::fs::write(&self.path, body).expect("write the gh stub");
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o755))
            .expect("make the gh stub executable");
        self.path
    }
}
