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

use crate::config::{DaemonConfig, UserMapping};

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
    config_path: &Path,
    github_user: &str,
    os_user: &str,
) -> Result<UserMapping, EnrolmentRefusal> {
    let not_writable = |reason: String| EnrolmentRefusal::ConfigNotWritable { reason };

    // Re-read rather than trust the caller's snapshot: the file on disk is what decides whether
    // this deployment has already enrolled somebody, including through a concurrent login.
    let config = DaemonConfig::load(config_path).map_err(|e| not_writable(e.to_string()))?;
    if let Some(enrolled) = config.users.first() {
        return Err(EnrolmentRefusal::AlreadyEnrolled {
            github_user: enrolled.github_user.clone(),
        });
    }

    let mapping = UserMapping {
        github_user: github_user.to_string(),
        os_user: os_user.to_string(),
    };

    // Edit the document rather than re-serialise the typed config, so every key the operator
    // wrote stays exactly as they wrote it and only `users:` changes.
    // TODO: comments in the file are still lost — preserving them needs a comment-aware YAML
    // editor, the same open question `DaemonConfigService`'s `write_config` records.
    let contents = std::fs::read_to_string(config_path)
        .map_err(|e| not_writable(format!("{}: {e}", config_path.display())))?;
    let mut document: serde_yaml::Value = serde_yaml::from_str(&contents)
        .map_err(|e| not_writable(format!("{}: {e}", config_path.display())))?;
    let users = serde_yaml::to_value(std::slice::from_ref(&mapping))
        .map_err(|e| not_writable(format!("failed to serialise the users row: {e}")))?;
    match &mut document {
        serde_yaml::Value::Mapping(root) => {
            root.insert(serde_yaml::Value::String("users".to_string()), users);
        }
        _ => {
            return Err(not_writable(format!(
                "{} is not a YAML mapping",
                config_path.display()
            )))
        }
    }
    let rewritten = serde_yaml::to_string(&document)
        .map_err(|e| not_writable(format!("failed to serialise the config: {e}")))?;
    tddy_core::atomic_file::write_atomic_labelled(config_path, rewritten).map_err(not_writable)?;
    Ok(mapping)
}

/// Install an enrolled row into a config already in memory.
///
/// Persisting alone is not enough: the daemon resolved `users:` when it started and holds a
/// snapshot, so a login enrolled at run time would be refused until the next restart. This is the
/// in-memory half, kept separate from the file write so the order is explicit — persist first,
/// then apply, so a daemon never admits a login it failed to record.
pub fn apply_enrolment(config: &mut DaemonConfig, mapping: UserMapping) {
    config.users.push(mapping);
}
