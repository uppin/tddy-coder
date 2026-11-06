//! Where a `tddy-tools` process finds the session it reports to (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! A headless coding agent's environment carries no `TDDY_*` variable, so the `tddy-tools` it runs
//! — from its shell (`tddy-tools submit …`, the command every workflow prompt names) or as its MCP
//! server, which runs in the agent's cwd — finds its session in this order:
//!
//! 1. an explicit `--socket <path>`;
//! 2. the nearest `.tddy/session.json` at or above its cwd, which the backend writes before each
//!    headless turn ([`tddy_core::session_pointer`]);
//! 3. `TDDY_SOCKET` — for the daemon's own callers and tests, never anything an agent inherits.
//!
//! None of the three is an error naming all three ([`SessionNotFound`]), never a silent local run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_core::session_pointer::SessionPointerOwner;

/// Which of the three sources named the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSource {
    /// `--socket <path>`.
    Arg,
    /// The pointer file at this path.
    PointerFile(PathBuf),
    /// `TDDY_SOCKET` (with `TDDY_SESSION_DIR`).
    Env,
}

/// The session a `tddy-tools` process reports to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSession {
    /// The toolcall socket; `None` for a pointer that carries only the notify session.
    pub socket: Option<PathBuf>,
    /// The repository the session works in, where the source names one (the pointer does).
    pub repo_dir: Option<PathBuf>,
    /// The session directory, where the source names one.
    pub session_dir: Option<PathBuf>,
    /// The remote tool environment the pointer carries, by variable name; empty otherwise.
    pub remote: BTreeMap<String, String>,
    /// What `tddy-tools notify` reads its session from, as the pointer carries it; empty otherwise.
    pub notify: BTreeMap<String, String>,
    pub source: SessionSource,
}

/// Why no session was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionNotFound {
    /// None of the three sources names one; `cwd` is where the pointer search started.
    Nowhere { cwd: PathBuf },
    /// A pointer file exists but cannot be used — never skipped for the next source.
    UnreadablePointer {
        path: PathBuf,
        cause: UnreadablePointerCause,
    },
}

/// Why a pointer file that exists names no session to report to (`#agent-tools 21/28`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreadablePointerCause {
    /// It could not be read, or does not parse.
    Invalid(String),
    /// Its owner runs on this host, and is no longer running.
    OwnerGone { owner: SessionPointerOwner },
    /// Its owner runs on this host, and the socket it names does not exist.
    SocketMissing { socket: PathBuf },
}

impl std::fmt::Display for UnreadablePointerCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnreadablePointerCause::Invalid(reason) => f.write_str(reason),
            UnreadablePointerCause::OwnerGone { owner } => write!(
                f,
                "the session that wrote it (pid {} on {}) is no longer running",
                owner.pid, owner.host
            ),
            UnreadablePointerCause::SocketMissing { socket } => {
                write!(
                    f,
                    "the socket it names, {}, does not exist",
                    socket.display()
                )
            }
        }
    }
}

impl std::fmt::Display for SessionNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionNotFound::Nowhere { cwd } => write!(
                f,
                "no session to report to: pass --socket <path>, run from a directory at or below \
                 a .tddy/session.json (searched upward from {}), or set TDDY_SOCKET",
                cwd.display()
            ),
            SessionNotFound::UnreadablePointer { path, cause } => write!(
                f,
                "the session pointer {} could not be read: {cause}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SessionNotFound {}

/// The session for a process given `explicit_socket` (`--socket`), running in `cwd`, whose
/// environment is `env`: the first of the three sources that names one.
pub fn resolve_session(
    explicit_socket: Option<&Path>,
    cwd: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<ResolvedSession, SessionNotFound> {
    if let Some(socket) = explicit_socket {
        return Ok(ResolvedSession {
            socket: Some(socket.to_path_buf()),
            repo_dir: None,
            session_dir: None,
            remote: BTreeMap::new(),
            notify: BTreeMap::new(),
            source: SessionSource::Arg,
        });
    }
    let pointer = tddy_core::session_pointer::find_session_pointer(cwd).map_err(|e| {
        SessionNotFound::UnreadablePointer {
            path: e.path,
            cause: UnreadablePointerCause::Invalid(e.reason),
        }
    })?;
    // TODO(session-pointer): implement — a pointer whose `owner` is on this host is refused when that
    // process is gone (`OwnerGone`) or the socket path it names is missing (`SocketMissing`); an
    // owner on another host is not checked here.
    if let Some((path, pointer)) = pointer {
        return Ok(ResolvedSession {
            socket: pointer.socket,
            repo_dir: Some(pointer.repo_dir),
            session_dir: pointer.session_dir,
            remote: pointer.remote,
            notify: pointer.notify,
            source: SessionSource::PointerFile(path),
        });
    }
    match env(SOCKET_VARIABLE).filter(|socket| !socket.is_empty()) {
        Some(socket) => Ok(ResolvedSession {
            socket: Some(PathBuf::from(socket)),
            repo_dir: None,
            session_dir: env("TDDY_SESSION_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
            remote: BTreeMap::new(),
            notify: BTreeMap::new(),
            source: SessionSource::Env,
        }),
        None => Err(SessionNotFound::Nowhere {
            cwd: cwd.to_path_buf(),
        }),
    }
}

/// The variable naming the session socket: the third source, for the daemon's own callers and tests.
const SOCKET_VARIABLE: &str = "TDDY_SOCKET";

/// The session this process reports to: [`resolve_session`] from its cwd and its environment.
pub fn resolve_current_session(
    explicit_socket: Option<&Path>,
) -> Result<ResolvedSession, SessionNotFound> {
    let cwd = std::env::current_dir().map_err(|e| SessionNotFound::UnreadablePointer {
        path: PathBuf::from("."),
        cause: UnreadablePointerCause::Invalid(format!(
            "the working directory cannot be read: {e}"
        )),
    })?;
    resolve_session(explicit_socket, &cwd, &|name| std::env::var(name).ok())
}

/// Makes `session` this process's own environment — `TDDY_SOCKET`, `TDDY_SESSION_DIR`,
/// `TDDY_REPO_DIR`, the `TDDY_REMOTE_*` pair and the notify session `tddy-tools notify` reads — so
/// the code that reads them (the MCP server's relays and path rules, the session tool client, the
/// notify command) finds the session whichever source named it.
/// Called once, at startup, before the async runtime starts a thread — writing the environment
/// while another thread reads it is unsound. The agent that ran this process never had these
/// variables, and a command it runs through the exec tools is started without them.
pub fn adopt_session(session: &ResolvedSession) {
    for (name, value) in session_environment(session) {
        std::env::set_var(name, value);
    }
}

/// The variables [`adopt_session`] sets for `session`. Of a pointer's `notify` map, only the names
/// `tddy-tools notify` reads: a pointer is a file found by walking up from the cwd, and must not be
/// able to set `PATH` or anything else in this process.
pub fn session_environment(session: &ResolvedSession) -> Vec<(String, String)> {
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let mut env = Vec::new();
    if let Some(socket) = &session.socket {
        env.push((SOCKET_VARIABLE.to_string(), path(socket)));
    }
    if let Some(dir) = &session.session_dir {
        env.push(("TDDY_SESSION_DIR".to_string(), path(dir)));
    }
    if let Some(repo) = &session.repo_dir {
        env.push(("TDDY_REPO_DIR".to_string(), path(repo)));
    }
    env.extend(session.remote.clone());
    env.extend(
        session
            .notify
            .iter()
            .filter(|(name, _)| NOTIFY_VARIABLES.contains(&name.as_str()))
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    env
}

/// What a pointer's `notify` may set: [`tddy_service::session_activity::notify_env`]'s names.
const NOTIFY_VARIABLES: [&str; 5] = [
    tddy_service::session_activity::notify_env::SESSION_ID,
    tddy_service::session_activity::notify_env::OS_USER,
    tddy_service::session_activity::notify_env::HOOK_TOKEN,
    tddy_service::session_activity::notify_env::DAEMON_URL,
    tddy_service::session_activity::notify_env::DAEMON_SOCKET,
];
