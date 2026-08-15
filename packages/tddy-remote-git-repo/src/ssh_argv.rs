//! Git's SSH argv contract.
//!
//! Git runs its transport command as `<ssh-command> [options] <host> <command>`, where `<command>`
//! is a single **shell-quoted** string (`git-upload-pack 'my-app'`). Which options git places ahead
//! of the host depends on the SSH variant it selected, so they are handled explicitly rather than
//! mistaken for the host: `-o <setting>` is accepted and ignored (including git's protocol-v2 probe
//! `-o SendEnv=GIT_PROTOCOL`, which is why the shim works under either variant), and every other
//! leading `-…` is refused by name — a daemon instance id is the whole address.
//!
//! See docs/ft/daemon/remote-git-repo.md § Client — git's SSH argv contract.

/// The git pack verbs this binary carries. A closed set — this is a git shell, not a shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVerb {
    /// `git-upload-pack` — clone, fetch, ls-remote.
    UploadPack,
    /// `git-receive-pack` — push.
    ReceivePack,
}

impl GitVerb {
    /// The canonical hyphenated name carried in `GitOpen.verb`, whichever spelling git used.
    pub fn wire_name(self) -> &'static str {
        match self {
            GitVerb::UploadPack => "git-upload-pack",
            GitVerb::ReceivePack => "git-receive-pack",
        }
    }
}

/// What git asked for, resolved from its argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRequest {
    /// The daemon's `daemon_instance_id`; its LiveKit participant is `daemon-{instance_id}`.
    pub daemon_instance_id: String,
    pub verb: GitVerb,
    /// The project's `name` or `project_id`. Never a filesystem path.
    pub project_ref: String,
}

/// Why an invocation could not be served, before any network call is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgvError {
    /// No `<host>` argument.
    MissingHost,
    /// A host but no `<command>`.
    MissingCommand,
    /// A command outside the git pack whitelist. Carries the rejected command verbatim.
    UnsupportedCommand(String),
    /// An unterminated or otherwise unparsable shell quote in the command.
    MalformedQuoting(String),
    /// An SSH option this shim cannot honour and must not silently drop. Carries the option as git
    /// passed it.
    UnsupportedOption(String),
    /// A command whose verb is recognised but which names no repository.
    MissingProjectRef,
}

impl ArgvError {
    /// The process exit code for this class of failure. `128` is git's own "fatal, bad request"
    /// code, distinct from `255`, which ssh reserves for transport failure.
    pub fn exit_code(&self) -> i32 {
        128
    }
}

impl std::fmt::Display for ArgvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgvError::MissingHost => write!(
                f,
                "no host given; expected `<daemon-instance-id> <git command>`"
            ),
            ArgvError::MissingCommand => write!(
                f,
                "no command given; this is a git remote, not an interactive shell"
            ),
            ArgvError::UnsupportedCommand(command) => write!(
                f,
                "refusing to run `{command}`: only git-upload-pack and git-receive-pack are served"
            ),
            ArgvError::MalformedQuoting(command) => {
                write!(f, "unterminated shell quote in `{command}`")
            }
            ArgvError::UnsupportedOption(option) => write!(
                f,
                "unsupported ssh option `{option}`: a daemon instance id is the whole address"
            ),
            ArgvError::MissingProjectRef => {
                write!(f, "the command names no project to serve")
            }
        }
    }
}

/// Parse the ssh-style tail git appends to the command: `[options] <host> <command>`.
///
/// `<host>` may carry an ignored `user@` prefix, so a habitual `git@daemon:project` remote works.
/// `<command>` is dequoted the way a login shell would dequote it before `ssh` execs it.
pub fn parse_ssh_invocation(argv: &[String]) -> Result<GitRequest, ArgvError> {
    let mut index = 0;
    while let Some(option) = argv.get(index) {
        match option.as_str() {
            // Settings for an ssh client that is not here. Dropping them — git's protocol-v2 probe
            // included — is what lets either SSH variant's argv resolve.
            "-o" => index += 2,
            // Anything else that leads with a hyphen is an ssh option this shim does not
            // understand. Treating it as the host instead would name the option as the daemon and
            // the daemon as the command, so the refusal would describe the wrong thing entirely.
            _ if option.starts_with('-') => {
                return Err(ArgvError::UnsupportedOption(option.clone()))
            }
            _ => break,
        }
    }

    let host = argv.get(index).ok_or(ArgvError::MissingHost)?;
    let command = argv.get(index + 1).ok_or(ArgvError::MissingCommand)?;

    let daemon_instance_id = match host.split_once('@') {
        Some((_user, host)) => host,
        None => host.as_str(),
    };

    let words = dequote_command(command)?;
    let (verb, operands) = match words.split_first() {
        Some((first, rest)) if first == "git-upload-pack" => (GitVerb::UploadPack, rest),
        Some((first, rest)) if first == "git-receive-pack" => (GitVerb::ReceivePack, rest),
        Some((first, rest)) if first == "git" => match rest.split_first() {
            Some((second, rest)) if second == "upload-pack" => (GitVerb::UploadPack, rest),
            Some((second, rest)) if second == "receive-pack" => (GitVerb::ReceivePack, rest),
            _ => return Err(ArgvError::UnsupportedCommand(command.clone())),
        },
        _ => return Err(ArgvError::UnsupportedCommand(command.clone())),
    };

    let project_ref = operands.first().ok_or(ArgvError::MissingProjectRef)?;
    // `host:/my-app` and `host:my-app` name the same project — there is no filesystem root to be
    // relative to, since the daemon resolves the ref against its own project registry.
    let project_ref = project_ref.strip_prefix('/').unwrap_or(project_ref);
    if project_ref.is_empty() {
        return Err(ArgvError::MissingProjectRef);
    }

    Ok(GitRequest {
        daemon_instance_id: daemon_instance_id.to_string(),
        verb,
        project_ref: project_ref.to_string(),
    })
}

/// Split a shell-quoted command string into its arguments, honouring single quotes, double quotes
/// and backslash escapes — the subset `ssh` receives from git (`git sq_quote`).
fn dequote_command(command: &str) -> Result<Vec<String>, ArgvError> {
    let unterminated = || ArgvError::MalformedQuoting(command.to_string());

    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = command.chars();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next().ok_or_else(unterminated)? {
                        '\'' => break,
                        c => word.push(c),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next().ok_or_else(unterminated)? {
                        '"' => break,
                        '\\' => word.push(chars.next().ok_or_else(unterminated)?),
                        c => word.push(c),
                    }
                }
            }
            // A bare backslash carries the next character literally — this is how git's sq_quote
            // spells an apostrophe inside a single-quoted word: `'it'\''s my app'`.
            '\\' => {
                in_word = true;
                word.push(chars.next().ok_or_else(unterminated)?);
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }

    if in_word {
        words.push(word);
    }
    Ok(words)
}
