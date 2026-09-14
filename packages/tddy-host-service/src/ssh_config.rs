//! Explicit OpenSSH `Host` aliases from an OS user's `~/.ssh/config`.
//!
//! Honesty differs from [`crate::host_private_key::list_key_candidates`]: an unreadable or
//! unparseable config is a **failure**, never an empty alias list. Empty means this user has no
//! explicit aliases, which is the LocalShell choice. Collapsing the two would silently force
//! local execution.
//!
//! Parser rules: honor `Include`; skip `Host` patterns that contain wildcards (`*` / `?`); list
//! each explicit alias once, in first-seen order. Does not shell out to `ssh -G`.

use crate::host_private_key::HostUserFiles;

/// Why a listing could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshConfigHostsError {
    /// The config (or an `Include`d file) could not be read or parsed.
    Unreadable(String),
}

/// Explicit aliases after `Include` expansion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SshConfigHostList {
    pub aliases: Vec<String>,
}

/// Explicit `Host` aliases in one config document. Does not expand `Include`.
///
/// Wildcard patterns are skipped. Repeated aliases appear once, first-seen order.
pub fn explicit_aliases_in(_config_text: &str) -> Result<Vec<String>, SshConfigHostsError> {
    // TODO(ssh-config): implement
    let _ = _config_text;
    Err(SshConfigHostsError::Unreadable(
        "TODO(ssh-config): implement".to_string(),
    ))
}

/// Aliases for `os_user`'s `~/.ssh/config`, including `Include`, read as that user.
pub fn list_ssh_config_hosts(
    _files: &dyn HostUserFiles,
    _os_user: &str,
) -> Result<SshConfigHostList, SshConfigHostsError> {
    // TODO(ssh-config): implement
    let _ = (_files, _os_user);
    Err(SshConfigHostsError::Unreadable(
        "TODO(ssh-config): implement".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_private_key::UserFilesUnder;

    #[test]
    fn lists_an_explicit_alias_and_skips_a_wildcard_host() {
        // Given a config that names a machine and a catch-all
        let config =
            "Host buildbox\n    HostName 10.0.0.8\nHost *\n    StrictHostKeyChecking accept-new\n";

        // When the aliases are listed
        let aliases = explicit_aliases_in(config).expect("a readable config lists");

        // Then only the explicit name is offered
        assert_eq!(aliases, vec!["buildbox".to_string()]);
    }

    #[test]
    fn lists_each_alias_once_in_first_seen_order() {
        // Given two names on one Host line, then a repeat
        let config = "Host jump buildbox\nHost buildbox\n";

        // When
        let aliases = explicit_aliases_in(config).expect("a readable config lists");

        // Then
        assert_eq!(aliases, vec!["jump".to_string(), "buildbox".to_string()]);
    }

    #[test]
    fn an_empty_config_is_no_aliases() {
        // Given
        let config = "";

        // When
        let aliases = explicit_aliases_in(config).expect("empty is readable");

        // Then
        assert!(aliases.is_empty());
    }

    #[test]
    fn honors_include_from_the_same_directory() {
        // Given a user whose config pulls in extra Host stanzas
        let home = tempfile::tempdir().expect("home");
        let ssh = home.path().join(".ssh");
        std::fs::create_dir_all(&ssh).expect("ssh dir");
        std::fs::write(ssh.join("extra"), "Host included-box\n").expect("included file");
        std::fs::write(ssh.join("config"), "Include extra\nHost local-box\n").expect("config");
        let files = UserFilesUnder::home(home.path());

        // When
        let listed = list_ssh_config_hosts(&files, "testdev").expect("readable Include");

        // Then both files contribute, Include first
        assert_eq!(
            listed.aliases,
            vec!["included-box".to_string(), "local-box".to_string()]
        );
    }
}
