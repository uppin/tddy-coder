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
//!
//! [`enrol_first_login`] is the file half. The running daemon enrols through
//! [`LiveUsers::enrol_first_login`](crate::live_users::LiveUsers::enrol_first_login), which
//! serialises it against every other enrolment and applies the row to the shared in-memory map
//! only once it is persisted — so a daemon never admits a login it failed to record.

use std::path::Path;

use crate::config::access::UserMapping;

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
    /// The mapping belongs to `tddy-supervisor` (`#agent-tools` 31/32): this daemon was handed
    /// `users.yaml` read-only, and a login is mapped by its operator editing that file.
    OwnedBySupervisor,
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
            Self::OwnedBySupervisor => write!(
                f,
                "this deployment's users.yaml belongs to tddy-supervisor; map a login by editing it"
            ),
        }
    }
}

impl std::error::Error for EnrolmentRefusal {}

/// Write the first row into `users.yaml` at `config_path`, mapping `github_user` to `os_user`, and
/// return the row that was written (`#agent-tools` 31/32: the mapping no longer lives in
/// `daemon.yaml`, which enrolment leaves untouched).
///
/// The write is atomic — a half-written mapping is a daemon that will not start.
pub fn enrol_first_login(
    config_path: &Path,
    github_user: &str,
    os_user: &str,
) -> Result<UserMapping, EnrolmentRefusal> {
    let not_writable = |reason: String| EnrolmentRefusal::ConfigNotWritable { reason };

    // `config_path` is the `users.yaml` to write (its caller derives it beside the daemon's
    // config). Re-read rather than trust a snapshot: the file on disk decides whether this
    // deployment has already enrolled somebody, including through a concurrent login. A file
    // that does not exist yet maps nobody — the installers render one, and enrolment is the
    // write that fills it.
    let existing: Vec<UserMapping> = match std::fs::read_to_string(config_path) {
        Ok(contents) if contents.trim().is_empty() => Vec::new(),
        Ok(contents) => serde_yaml::from_str(&contents)
            .map_err(|e| not_writable(format!("{}: {e}", config_path.display())))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(not_writable(format!(
                "{}: {error}",
                config_path.display()
            )))
        }
    };
    if let Some(enrolled) = existing.first() {
        return Err(EnrolmentRefusal::AlreadyEnrolled {
            github_user: enrolled.github_user.clone(),
        });
    }

    // The atomic write creates missing directories; enrolment must not resurrect a deployment
    // whose directory has gone — that deployment is not there to enrol anybody into.
    if let Some(directory) = config_path.parent() {
        if !directory.as_os_str().is_empty() && !directory.is_dir() {
            return Err(not_writable(format!(
                "{}: its directory {} is gone",
                config_path.display(),
                directory.display()
            )));
        }
    }
    let mapping = UserMapping {
        github_user: github_user.to_string(),
        os_user: os_user.to_string(),
    };
    let rewritten = serde_yaml::to_string(std::slice::from_ref(&mapping))
        .map_err(|e| not_writable(format!("failed to serialise the users row: {e}")))?;
    tddy_core::atomic_file::write_atomic_labelled(config_path, rewritten).map_err(not_writable)?;
    Ok(mapping)
}
