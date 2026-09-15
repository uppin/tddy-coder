//! Explicit OpenSSH `Host` aliases from an OS user's `~/.ssh/config`.
//!
//! Honesty differs from [`crate::host_private_key::list_key_candidates`]: an unreadable or
//! unparseable config is a **failure**, never an empty alias list. Empty means this user has no
//! explicit aliases, which is the LocalShell choice. Collapsing the two would silently force
//! local execution.
//!
//! Parser rules: honor `Include`; skip `Host` patterns that contain wildcards (`*` / `?`); list
//! each explicit alias once, in first-seen order. Does not shell out to `ssh -G`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

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

const SSH_DIR: &str = ".ssh";
const CONFIG_FILE: &str = "config";

/// Explicit `Host` aliases in one config document. Does not expand `Include`.
///
/// Wildcard patterns are skipped. Repeated aliases appear once, first-seen order.
pub fn explicit_aliases_in(config_text: &str) -> Result<Vec<String>, SshConfigHostsError> {
    let mut aliases = Vec::new();
    for line in config_text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        if !parts[0].eq_ignore_ascii_case("host") {
            continue;
        }
        let patterns = &parts[1..];
        if patterns
            .iter()
            .any(|pattern| pattern.contains('*') || pattern.contains('?'))
        {
            continue;
        }
        for pattern in patterns {
            if !aliases.iter().any(|seen| seen == pattern) {
                aliases.push(pattern.to_string());
            }
        }
    }
    Ok(aliases)
}

/// Aliases for `os_user`'s `~/.ssh/config`, including `Include`, read as that user.
pub fn list_ssh_config_hosts(
    files: &dyn HostUserFiles,
    os_user: &str,
) -> Result<SshConfigHostList, SshConfigHostsError> {
    let home = files
        .home_dir(os_user)
        .map_err(SshConfigHostsError::Unreadable)?;
    let config_path = home.join(SSH_DIR).join(CONFIG_FILE);
    let aliases =
        aliases_from_config_file(files, os_user, &config_path, &mut HashSet::new(), true)?;
    Ok(SshConfigHostList { aliases })
}

fn aliases_from_config_file(
    files: &dyn HostUserFiles,
    os_user: &str,
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    optional_if_missing: bool,
) -> Result<Vec<String>, SshConfigHostsError> {
    let bytes = match files.read_as_user(os_user, path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            if optional_if_missing && config_read_is_absent(&reason) {
                return Ok(Vec::new());
            }
            return Err(SshConfigHostsError::Unreadable(reason));
        }
    };
    let canonical = path
        .canonicalize()
        .map_err(|e| SshConfigHostsError::Unreadable(e.to_string()))?;
    if !visited.insert(canonical) {
        return Ok(Vec::new());
    }
    let text =
        String::from_utf8(bytes).map_err(|e| SshConfigHostsError::Unreadable(e.to_string()))?;

    let parent = path
        .parent()
        .ok_or_else(|| SshConfigHostsError::Unreadable("config path has no parent".to_string()))?;

    let mut aliases = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        if parts[0].eq_ignore_ascii_case("include") {
            for pattern in &parts[1..] {
                let included = resolve_include_path(parent, pattern);
                let from_included =
                    aliases_from_config_file(files, os_user, &included, visited, false)?;
                merge_aliases(&mut aliases, from_included);
            }
            continue;
        }
        if parts[0].eq_ignore_ascii_case("host") && parts.len() >= 2 {
            let patterns = &parts[1..];
            if patterns
                .iter()
                .any(|pattern| pattern.contains('*') || pattern.contains('?'))
            {
                continue;
            }
            for pattern in patterns {
                if !aliases.iter().any(|seen| seen == pattern) {
                    aliases.push(pattern.to_string());
                }
            }
        }
    }
    Ok(aliases)
}

fn merge_aliases(into: &mut Vec<String>, more: Vec<String>) {
    for alias in more {
        if !into.iter().any(|seen| seen == &alias) {
            into.push(alias);
        }
    }
}

fn resolve_include_path(config_dir: &Path, pattern: &str) -> PathBuf {
    let path = Path::new(pattern);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir.join(path)
    }
}

fn config_read_is_absent(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("no such file") || lower.contains("not found")
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
