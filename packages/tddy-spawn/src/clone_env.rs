//! The environment a daemon-run SSH clone is given, and whether the host's OpenSSH can honour it.
//!
//! Composed by a pure function so what reaches `git` is a value a test can read, the same whichever
//! backend carries it. Under a supervisor, every key [`CloneSshEnv::into_env`] can set must be on
//! `spawn_policy.allowed_env_keys`; [`CloneSshEnv::ALLOWED_ENV_KEYS`] is that list.

use std::path::PathBuf;
use std::time::Duration;

/// How long ssh waits for a silent host before giving up, unless the caller chooses otherwise.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// The SSH settings of one clone: ask the operator for a passphrase through the helper, and keep
/// the unlocked key in the user's agent.
#[derive(Debug, Clone)]
pub struct CloneSshEnv {
    askpass_program: PathBuf,
    askpass_url: String,
    askpass_token: String,
    agent_socket: Option<PathBuf>,
    ssh_args: Vec<String>,
    connect_timeout: Duration,
    prompting: bool,
}

impl CloneSshEnv {
    /// Every key [`Self::into_env`] can set, sorted. This is what a supervisor's
    /// `allowed_env_keys` must contain for a clone to be accepted.
    pub const ALLOWED_ENV_KEYS: &'static [&'static str] = &[
        "GIT_SSH_COMMAND",
        "GIT_TERMINAL_PROMPT",
        "SSH_ASKPASS",
        "SSH_ASKPASS_REQUIRE",
        "SSH_AUTH_SOCK",
        "TDDY_ASKPASS_TOKEN",
        "TDDY_ASKPASS_URL",
    ];

    /// A clone whose ssh asks `askpass_program`, which calls the daemon at `askpass_url` with
    /// `askpass_token`.
    pub fn new(
        askpass_program: impl Into<PathBuf>,
        askpass_url: &str,
        askpass_token: &str,
    ) -> Self {
        Self {
            askpass_program: askpass_program.into(),
            askpass_url: askpass_url.to_string(),
            askpass_token: askpass_token.to_string(),
            agent_socket: None,
            ssh_args: Vec::new(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            prompting: true,
        }
    }

    /// The user's ssh-agent socket. Left unset otherwise: an empty `SSH_AUTH_SOCK` is a different
    /// failure from no agent.
    #[must_use]
    pub fn agent_socket(mut self, socket: impl Into<PathBuf>) -> Self {
        self.agent_socket = Some(socket.into());
        self
    }

    /// Extra arguments for `ssh`, after the ones every clone gets. Each is quoted for the shell git
    /// runs the command through.
    ///
    /// FIXME(clone-ssh-options): no production caller yet. The real-process acceptance suite uses it to
    /// aim ssh at a fixture server; an operator-configured set of ssh options would be its first
    /// production use.
    #[must_use]
    pub fn ssh_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ssh_args = args.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// A clone that may not ask for anything: ssh fails fast (`BatchMode=yes`) where it would have
    /// prompted, and the helper variables are left out. For a host whose OpenSSH cannot be told to
    /// use a helper without a display (see [`OpenSshVersion::supports_forced_askpass`]), where the
    /// alternative is a clone that hangs on a passphrase nobody is shown.
    #[must_use]
    pub fn without_prompting(mut self) -> Self {
        self.prompting = false;
        self
    }

    /// The variables to set on the `git clone` child.
    #[must_use]
    pub fn into_env(self) -> Vec<(String, String)> {
        let mut ssh_command = format!(
            "ssh -o AddKeysToAgent=yes -o ConnectTimeout={}",
            self.connect_timeout.as_secs()
        );
        if !self.prompting {
            ssh_command.push_str(" -o BatchMode=yes");
        }
        for arg in &self.ssh_args {
            ssh_command.push(' ');
            ssh_command.push_str(&shell_word(arg));
        }

        let mut env = vec![
            ("GIT_SSH_COMMAND".to_string(), ssh_command),
            ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
        ];
        if self.prompting {
            env.extend([
                (
                    "SSH_ASKPASS".to_string(),
                    self.askpass_program.display().to_string(),
                ),
                // Without `force`, ssh calls its helper only when DISPLAY is set, and a daemon has none.
                ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
                ("TDDY_ASKPASS_TOKEN".to_string(), self.askpass_token),
                ("TDDY_ASKPASS_URL".to_string(), self.askpass_url),
            ]);
        }
        if let Some(socket) = self.agent_socket {
            env.push(("SSH_AUTH_SOCK".to_string(), socket.display().to_string()));
        }
        env
    }
}

/// An OpenSSH release, as its `ssh -V` banner names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpenSshVersion {
    major: u32,
    minor: u32,
}

impl OpenSshVersion {
    /// Parse a banner such as `OpenSSH_8.3p1 Ubuntu-1ubuntu0.2, OpenSSL 1.1.1f`. `None` for
    /// anything that is not OpenSSH's own banner.
    #[must_use]
    pub fn parse(banner: &str) -> Option<Self> {
        let rest = banner.trim_start().strip_prefix("OpenSSH_")?;
        let (major, rest) = leading_number(rest)?;
        let (minor, _) = leading_number(rest.strip_prefix('.')?)?;
        Some(Self { major, minor })
    }

    /// `SSH_ASKPASS_REQUIRE=force` exists from OpenSSH 8.4; before it ssh asks for a helper only
    /// when `DISPLAY` is set.
    #[must_use]
    pub fn supports_forced_askpass(&self) -> bool {
        *self >= Self { major: 8, minor: 4 }
    }
}

fn leading_number(text: &str) -> Option<(u32, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

/// `arg` as one word of the command line git hands to a shell for `GIT_SSH_COMMAND`.
///
/// Left alone when it is made only of characters a shell never splits or expands, so the common
/// arguments stay readable in a process listing; single-quoted otherwise, with a quote inside it
/// closed, escaped and reopened.
fn shell_word(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}
