use super::DaemonSessionHost;
use tddy_service::proto::session::session_attachment::Source as AttachmentSource;

use super::super::cleanup_materialized_attachments;

use super::super::attachment_size_bytes;

use super::super::AttachmentProgressReporter;

use crate::session_attachments::validate_attachment_basename;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;
use tddy_service::proto::session::SessionAttachment;

use super::super::AttachmentMaterialization;

impl DaemonSessionHost {
    /// Pre-creates `session_dir` when needed and materializes the request's attachments before spawn.
    ///
    /// Answers with the attachments that reached the session's store, which is what a caller
    /// deriving anything from them — the pr-stack changeset the child's prompt names, say — must
    /// read: the request says what was asked for, this says what the child actually holds.
    pub(crate) async fn prepare_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        if ctx.attachments.is_empty() {
            return Ok(Vec::new());
        }
        let session_dir = ctx.session_dir();
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
        self.materialize_session_attachments(ctx).await
    }

    pub(crate) async fn materialize_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        if ctx.attachments.is_empty() {
            return Ok(Vec::new());
        }

        let session_dir = ctx.session_dir();
        let local_instance_id = local_instance_id_for_config(&self.config);
        let mut seen_basenames = std::collections::HashSet::new();
        for att in ctx.attachments {
            let safe = validate_attachment_basename(&att.basename)?;
            if !seen_basenames.insert(safe.to_string()) {
                return Err(Status::invalid_argument(
                    "duplicate attachment basename in request",
                ));
            }
        }

        let staging_root = crate::session_attachment_staging::staging_root_for(
            ctx.os_user,
            &self.staging_base_dir,
        );
        let mut written: Vec<SessionAttachment> = Vec::new();
        let attachment_count = ctx.attachments.len() as u32;

        for (index, att) in ctx.attachments.iter().enumerate() {
            let basename = validate_attachment_basename(&att.basename)?.to_string();
            let source = att
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("attachment source must be set"))?;
            let reporter = AttachmentProgressReporter {
                sink: ctx.progress,
                basename: &basename,
                attachment_index: index as u32,
                attachment_count,
            };

            let materialize_result = match source {
                AttachmentSource::Staged(staged) => {
                    self.materialize_staged_attachment(
                        ctx.session_token,
                        &staging_root,
                        &session_dir,
                        staged,
                        &basename,
                        &reporter,
                    )
                    .await
                }
                AttachmentSource::HostDocument(host_doc) => {
                    self.materialize_host_document_attachment(
                        ctx.session_token,
                        ctx.os_user,
                        &session_dir,
                        host_doc,
                        &basename,
                        &local_instance_id,
                    )
                    .await
                }
            };

            match materialize_result {
                Ok(()) => {
                    // The attachment is on disk now, so its final size is the honest byte count to
                    // report — and it is the only report a source that copies in one step makes.
                    let bytes = attachment_size_bytes(&session_dir, &basename);
                    reporter.report(bytes, bytes);
                    // Under the basename it was written as, not the one that was requested — the
                    // two differ whenever validation trimmed it.
                    written.push(SessionAttachment {
                        basename,
                        source: att.source.clone(),
                    });
                }
                Err(e) => {
                    cleanup_materialized_attachments(&session_dir, &written);
                    return Err(e);
                }
            }
        }

        Ok(written)
    }
}
