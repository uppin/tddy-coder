use crate::config::ROOT_UID;
use crate::error::ConfigError;
use std::path::Path;

/// Refuse a policy file that somebody other than the supervisor's own identity could have written.
///
/// "Owned by uid 0 **or** by the supervisor's own effective uid" is one rule, not a concession: on a
/// host the supervisor runs as root, both halves are uid 0, and the rule reads "root-owned". Run
/// unprivileged — a developer, or the acceptance suite driving the real binary — it reads "owned by
/// whoever is enforcing it", which is the same statement about the same trust relationship. What it
/// never permits is a policy file authored by a *third* party, which is the only case that matters.
pub(crate) fn require_trustworthy_config_file(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: reads this process's own effective uid and nothing else.
    let enforcing_uid = unsafe { libc::geteuid() };
    if metadata.uid() != ROOT_UID && metadata.uid() != enforcing_uid {
        return Err(ConfigError {
            message: format!(
                "{} is owned by uid {} but only uid {ROOT_UID} or uid {enforcing_uid} may write the \
                 policy this supervisor enforces",
                path.display(),
                metadata.uid()
            ),
        });
    }
    if metadata.mode() & GROUP_OR_OTHER_WRITE != 0 {
        return Err(ConfigError {
            message: format!(
                "{} is writable by its group or by everyone (mode {:o}); anyone who can write it \
                 chooses what the supervisor permits",
                path.display(),
                metadata.mode() & 0o7777
            ),
        });
    }
    Ok(())
}

/// Refuse a directory on the way to the policy file that a third party could write.
///
/// Group- or world-writable is refused *unless* the sticky bit is set, and that exception is the
/// kernel's own rule rather than a loophole: in a sticky directory only the owner of an entry may
/// rename or unlink it, so `/tmp` being `1777` does not let anybody replace somebody else's file.
/// A `/etc/tddy` left at `0777` has no such protection, and that is precisely the case being caught.
pub(crate) fn require_trustworthy_config_directory(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<(), ConfigError> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: reads this process's own effective uid and nothing else.
    let enforcing_uid = unsafe { libc::geteuid() };
    if metadata.uid() != ROOT_UID && metadata.uid() != enforcing_uid {
        return Err(ConfigError {
            message: format!(
                "{} is owned by uid {}, so uid {} could replace the supervisor's policy file",
                path.display(),
                metadata.uid(),
                metadata.uid()
            ),
        });
    }
    let sticky = metadata.mode() & 0o1000 != 0;
    if metadata.mode() & GROUP_OR_OTHER_WRITE != 0 && !sticky {
        return Err(ConfigError {
            message: format!(
                "{} is writable by its group or by everyone (mode {:o}) and is not sticky, so the \
                 supervisor's policy file could be replaced",
                path.display(),
                metadata.mode() & 0o7777
            ),
        });
    }
    Ok(())
}

/// The two write bits that decide whether anybody but the owner can change something.
const GROUP_OR_OTHER_WRITE: u32 = 0o022;
