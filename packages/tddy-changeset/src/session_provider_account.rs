//! The model-provider account a session's harness runs on, when it runs on one.
//!
//! Stored as `provider-account.yaml` beside `.session.yaml`, not inside it, for the reasons
//! [`crate::session_github_source`] gives: `.session.yaml` is `deny_unknown_fields`, and for a tool
//! session it is the child's to write while this is the daemon's, before the child exists.
//!
//! Only a session started on an account writes the file; holds the account **id**, never a key.
//! A resume re-reads the key from the owner's vault, so a key rotated or removed since the start is
//! what the resume sees.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file this module reads and writes, inside a session directory.
pub const SESSION_PROVIDER_ACCOUNT_FILENAME: &str = "provider-account.yaml";

/// The account a session runs on, and the model its harness is pinned to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderAccount {
    pub account_id: String,
    pub model: String,
}

/// Record `account` for the session in `session_dir`, replacing any earlier record.
pub fn write_session_provider_account(
    session_dir: &Path,
    account: &SessionProviderAccount,
) -> Result<(), crate::WorkflowError> {
    let yaml = serde_yaml::to_string(account).map_err(|e| {
        crate::WorkflowError::WriteFailed(format!("{SESSION_PROVIDER_ACCOUNT_FILENAME}: {e}"))
    })?;
    std::fs::write(session_dir.join(SESSION_PROVIDER_ACCOUNT_FILENAME), yaml).map_err(|e| {
        crate::WorkflowError::WriteFailed(format!("{SESSION_PROVIDER_ACCOUNT_FILENAME}: {e}"))
    })
}

/// The account recorded for the session in `session_dir`; `None` when it runs on its vendor.
pub fn read_session_provider_account(
    session_dir: &Path,
) -> Result<Option<SessionProviderAccount>, crate::WorkflowError> {
    let path = session_dir.join(SESSION_PROVIDER_ACCOUNT_FILENAME);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(crate::WorkflowError::ChangesetInvalid(format!(
                "{}: {e}",
                path.display()
            )))
        }
    };
    serde_yaml::from_str(&contents)
        .map(Some)
        .map_err(|e| crate::WorkflowError::ChangesetInvalid(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_on_its_vendor_reads_as_no_account() {
        // Given a session directory holding no account record
        let session_dir = tempfile::tempdir().unwrap();

        // When its account is read
        let account =
            read_session_provider_account(session_dir.path()).expect("an absent record reads");

        // Then there is none
        assert_eq!(account, None);
    }

    #[test]
    fn a_recorded_account_reads_back_as_written() {
        // Given a session started on Ada's Meta account
        let session_dir = tempfile::tempdir().unwrap();
        let recorded = SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        };

        // When it is written and read back
        write_session_provider_account(session_dir.path(), &recorded).expect("written");
        let read = read_session_provider_account(session_dir.path()).expect("read");

        // Then it is the same account and model
        assert_eq!(read, Some(recorded));
    }

    #[test]
    fn the_record_holds_no_secret_shaped_field() {
        // Given a recorded account
        let session_dir = tempfile::tempdir().unwrap();
        write_session_provider_account(
            session_dir.path(),
            &SessionProviderAccount {
                account_id: "meta-hackathon".to_string(),
                model: "muse-spark-1.3".to_string(),
            },
        )
        .expect("written");

        // When the file is read as text
        let text =
            std::fs::read_to_string(session_dir.path().join(SESSION_PROVIDER_ACCOUNT_FILENAME))
                .expect("the file exists");

        // Then it names the account and model only
        assert!(text.contains("meta-hackathon"));
        assert!(!text.to_lowercase().contains("key"));
        assert!(!text.to_lowercase().contains("token"));
    }
}
