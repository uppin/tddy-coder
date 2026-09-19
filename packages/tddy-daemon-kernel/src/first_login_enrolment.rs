//! The one-time write that gives a desktop deployment its `users:` row.
//!
//! A server deployment is configured by whoever installs it: `users:` maps each GitHub login to
//! the OS user its sessions run as, and [`DaemonConfig::os_user_for_github`] returns `None` for
//! anyone absent — deliberately, with no default arm, because that mapping is what stops an
//! arbitrary GitHub account driving someone else's machine.
//!
//! A desktop install has nobody to write it. `./install --desktop` renders a config with `users:`
//! unset, so the first login is refused `permission_denied: user not mapped to OS user`, and the
//! application opens on a settings screen it cannot get past.
//!
//! **Enrolment is not a fallback for that lookup, and must never become one.** A default arm would
//! answer "no mapping" every time, for every GitHub account on earth. Instead this writes the row
//! *once*, on a deployment that has none, and the unchanged lookup then finds it — so a second,
//! different login on an enrolled deployment is refused exactly as it is today. Deliberately adding
//! a second account is `#keyring` 8/9's, through a path the operator asks for.

use std::path::Path;

use crate::config::UserMapping;

/// Why a deployment cannot enrol a first login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrolmentRefusal {
    /// `users:` already names somebody. Enrolment is once per deployment, and this is the shape
    /// that would otherwise silently admit a second GitHub account.
    AlreadyEnrolled { github_user: String },
    /// The config file this daemon was started from cannot be rewritten — read-only, owned by
    /// another account, or on a path that no longer exists. Reported rather than skipped: a login
    /// that appeared to work and vanished on restart is worse than one that refused.
    ConfigNotWritable { reason: String },
}

impl std::fmt::Display for EnrolmentRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyEnrolled { github_user } => write!(
                f,
                "this deployment is already enrolled to {github_user}; a second GitHub account is \
                 added deliberately, never by signing in"
            ),
            Self::ConfigNotWritable { reason } => {
                write!(f, "the daemon config cannot be rewritten: {reason}")
            }
        }
    }
}

impl std::error::Error for EnrolmentRefusal {}

/// Whether this deployment has never enrolled anyone — the only state enrolment may act on.
pub fn is_unenrolled(users: &[UserMapping]) -> bool {
    users.is_empty()
}

/// Write the first `users:` row into the config file at `config_path`, mapping `github_user` to
/// `os_user`, and return the row that was written.
///
/// The file is rewritten in place, preserving everything else it holds, and the write is atomic —
/// a half-written config is a daemon that will not start.
pub fn enrol_first_login(
    _config_path: &Path,
    _github_user: &str,
    _os_user: &str,
) -> Result<UserMapping, EnrolmentRefusal> {
    // TODO(desktop-login): re-read the file, refuse when `users:` is non-empty, append the row,
    // and write it back through `tddy_core::atomic_file`. The running daemon's own
    // `Arc<DaemonConfig>` is a separate question — see `apply_enrolment`.
    todo!("enrol_first_login")
}

/// Install an enrolled row into a config already in memory.
///
/// Persisting alone is not enough: the daemon resolved `users:` when it started and holds a
/// snapshot, so a login enrolled at run time would be refused until the next restart. This is the
/// in-memory half, kept separate from the file write so the order is explicit — persist first,
/// then apply, so a daemon never admits a login it failed to record.
pub fn apply_enrolment(_config: &mut crate::config::DaemonConfig, _mapping: UserMapping) {
    // TODO(desktop-login): push the row onto `config.users`. How the running daemon's shared
    // `Arc<DaemonConfig>` picks it up is the auth side's to wire.
    todo!("apply_enrolment")
}
