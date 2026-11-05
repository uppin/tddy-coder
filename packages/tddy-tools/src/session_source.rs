//! Where a `tddy-tools` process finds the session it reports to (`#agent-tools 15/19`).
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-headless-driver.md`.
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
    pub socket: PathBuf,
    /// The session directory, where the source names one.
    pub session_dir: Option<PathBuf>,
    /// The remote tool environment the pointer carries, by variable name; empty otherwise.
    pub remote: BTreeMap<String, String>,
    pub source: SessionSource,
}

/// Why no session was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionNotFound {
    /// None of the three sources names one; `cwd` is where the pointer search started.
    Nowhere { cwd: PathBuf },
    /// A pointer file exists but cannot be read — never skipped for the next source.
    UnreadablePointer { path: PathBuf, reason: String },
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
            SessionNotFound::UnreadablePointer { path, reason } => write!(
                f,
                "the session pointer {} could not be read: {reason}",
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
            socket: socket.to_path_buf(),
            session_dir: None,
            remote: BTreeMap::new(),
            source: SessionSource::Arg,
        });
    }
    let pointer = tddy_core::session_pointer::find_session_pointer(cwd).map_err(|e| {
        SessionNotFound::UnreadablePointer {
            path: e.path,
            reason: e.reason,
        }
    })?;
    if let Some((path, pointer)) = pointer {
        return Ok(ResolvedSession {
            socket: pointer.socket,
            session_dir: pointer.session_dir,
            remote: pointer.remote,
            source: SessionSource::PointerFile(path),
        });
    }
    match env(SOCKET_VARIABLE).filter(|socket| !socket.is_empty()) {
        Some(socket) => Ok(ResolvedSession {
            socket: PathBuf::from(socket),
            session_dir: env("TDDY_SESSION_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
            remote: BTreeMap::new(),
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
        reason: format!("the working directory cannot be read: {e}"),
    })?;
    resolve_session(explicit_socket, &cwd, &|name| std::env::var(name).ok())
}

/// Makes `session` this process's own environment — `TDDY_SOCKET`, `TDDY_SESSION_DIR`,
/// `TDDY_REPO_DIR` and the `TDDY_REMOTE_*` pair — so the code that reads them (the MCP server's
/// relays and path rules, the session tool client) finds the session whichever source named it.
/// Called once, at startup, before any other thread reads the environment; the agent that ran
/// this process never had these variables.
pub fn adopt_session(session: &ResolvedSession) {
    std::env::set_var(SOCKET_VARIABLE, &session.socket);
    if let Some(dir) = &session.session_dir {
        std::env::set_var("TDDY_SESSION_DIR", dir);
    }
    if let SessionSource::PointerFile(path) = &session.source {
        // The pointer sits at `<repo>/.tddy/session.json`.
        if let Some(repo) = path.parent().and_then(Path::parent) {
            std::env::set_var("TDDY_REPO_DIR", repo);
        }
    }
    for (name, value) in &session.remote {
        std::env::set_var(name, value);
    }
}
