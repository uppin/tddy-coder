//! Validate and materialize `StartSessionRequest.attachments` into `{session_dir}/attachments/`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use tddy_rpc::Status;
use tddy_service::proto::connection::{session_attachment, SessionAttachment, StagedAttachmentRef};

use crate::session_file_upload::validate_segment;
use crate::staged_attachment_upload::staging_dir_for;

/// Validates every attachment descriptor before session start. Rejects duplicate basenames, unsafe
/// basenames, unset sources, and a `StagedAttachmentRef` that names a foreign host.
pub fn validate_start_session_attachments(
    attachments: &[SessionAttachment],
    local_daemon_instance_id: &str,
) -> Result<(), Status> {
    let mut basenames = HashSet::new();
    for attachment in attachments {
        let basename = attachment.basename.trim();
        if basename.is_empty() {
            return Err(Status::invalid_argument(
                "attachment basename must not be empty",
            ));
        }
        validate_segment(basename).map_err(|_| {
            Status::invalid_argument("attachment basename must be a single path segment")
        })?;
        if !basenames.insert(basename.to_string()) {
            return Err(Status::invalid_argument(format!(
                "duplicate attachment basename {basename:?}"
            )));
        }

        let Some(source) = &attachment.source else {
            return Err(Status::invalid_argument(
                "attachment source must be set",
            ));
        };

        match source {
            session_attachment::Source::Staged(staged) => {
                validate_staged_ref(staged, local_daemon_instance_id)?;
            }
            session_attachment::Source::HostDocument(_) => {
                return Err(Status::unimplemented(
                    "HostDocumentRef attachments are not implemented yet",
                ));
            }
        }
    }
    Ok(())
}

fn validate_staged_ref(staged: &StagedAttachmentRef, local_daemon_instance_id: &str) -> Result<(), Status> {
    let requested = staged.daemon_instance_id.trim();
    if !requested.is_empty() && requested != local_daemon_instance_id {
        return Err(Status::invalid_argument(format!(
            "staged attachment batch is on host {requested:?}, but the session starts on {local_daemon_instance_id:?}"
        )));
    }
    validate_segment(staged.staging_id.trim()).map_err(|_| {
        Status::invalid_argument("staging_id must be a basename")
    })?;
    validate_segment(staged.file_name.trim()).map_err(|_| {
        Status::invalid_argument("file_name must be a basename")
    })?;
    Ok(())
}

/// Copies staged attachments into `{session_dir}/attachments/<basename>` before the agent launches.
pub fn materialize_start_session_attachments(
    sessions_base: &Path,
    session_dir: &Path,
    attachments: &[SessionAttachment],
) -> Result<(), Status> {
    if attachments.is_empty() {
        return Ok(());
    }

    let dest_root = session_dir.join("attachments");
    fs::create_dir_all(&dest_root).map_err(|e| {
        log::error!("materialize_start_session_attachments: create_dir_all {dest_root:?} failed: {e}");
        Status::internal(format!("failed to create attachments dir: {e}"))
    })?;

    for attachment in attachments {
        let basename = attachment.basename.trim();
        let session_attachment::Source::Staged(staged) = attachment
            .source
            .as_ref()
            .expect("validate_start_session_attachments already checked source")
        else {
            continue;
        };

        let source_path = staging_dir_for(sessions_base, staged.staging_id.trim())
            .join(staged.file_name.trim());
        if !source_path.is_file() {
            return Err(Status::not_found(format!(
                "staged attachment not found: {}",
                source_path.display()
            )));
        }

        let dest = dest_root.join(basename);
        fs::copy(&source_path, &dest).map_err(|e| {
            log::error!(
                "materialize_start_session_attachments: copy {source_path:?} -> {dest:?} failed: {e}"
            );
            Status::internal(format!("failed to materialize attachment: {e}"))
        })?;
        log::info!(
            "materialize_start_session_attachments: copied {} -> {}",
            source_path.display(),
            dest.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        materialize_start_session_attachments, validate_start_session_attachments,
    };
    use crate::staged_attachment_upload::write_staged_chunk;
    use std::fs;
    use tddy_service::proto::connection::{
        session_attachment, SessionAttachment, StagedAttachmentRef,
    };

    const LOCAL_DAEMON: &str = "daemon-local";
    const STAGING_ID: &str = "33333333-3333-7333-8333-333333333333";

    fn staged_attachment(basename: &str, file_name: &str) -> SessionAttachment {
        SessionAttachment {
            basename: basename.to_string(),
            source: Some(session_attachment::Source::Staged(StagedAttachmentRef {
                daemon_instance_id: LOCAL_DAEMON.to_string(),
                staging_id: STAGING_ID.to_string(),
                file_name: file_name.to_string(),
            })),
        }
    }

    #[test]
    fn rejects_duplicate_basenames_in_one_request() {
        let attachments = vec![
            staged_attachment("brief.md", "upload-a.md"),
            staged_attachment("brief.md", "upload-b.md"),
        ];
        let err = validate_start_session_attachments(&attachments, LOCAL_DAEMON)
            .expect_err("duplicate basenames must be rejected");
        assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
    }

    #[test]
    fn rejects_a_staged_ref_on_a_foreign_host() {
        let attachments = vec![SessionAttachment {
            basename: "brief.md".to_string(),
            source: Some(session_attachment::Source::Staged(StagedAttachmentRef {
                daemon_instance_id: "daemon-remote".to_string(),
                staging_id: STAGING_ID.to_string(),
                file_name: "brief.md".to_string(),
            })),
        }];
        let err = validate_start_session_attachments(&attachments, LOCAL_DAEMON)
            .expect_err("foreign staged ref must be rejected");
        assert_eq!(err.code, tddy_rpc::Code::InvalidArgument);
    }

    #[test]
    fn materializes_staged_bytes_into_the_session_attachments_directory() {
        let base = tempfile::tempdir().unwrap();
        write_staged_chunk(
            base.path(),
            STAGING_ID,
            "source.md",
            b"# Context",
            true,
        )
        .unwrap();

        let session_dir = base.path().join("sessions").join("sess-1");
        fs::create_dir_all(&session_dir).unwrap();

        materialize_start_session_attachments(
            base.path(),
            &session_dir,
            &[staged_attachment("brief.md", "source.md")],
        )
        .unwrap();

        let dest = session_dir.join("attachments").join("brief.md");
        assert_eq!(fs::read_to_string(dest).unwrap(), "# Context");
    }
}
