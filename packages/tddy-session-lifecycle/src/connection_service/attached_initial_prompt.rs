//! The first prompt of a CLI agent session, with the request's attachments materialized into it.
//!
//! Read by the launch topic's CLI starts and by the split topic's agent spawn, so it sits in the
//! split topic, which the launch topic calls: the other direction would make the split topic call
//! up. It reads the attachment fields only ([`AttachmentState`]), never the host.

use std::path::Path;

use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;
use tddy_session_files::attachment_progress::{AttachmentMaterialization, AttachmentProgressSink};

use super::svc_materialize_staged_attachment::AttachmentState;

/// Materialize the request's attachments into the session, and return its first prompt with a
/// line naming the attached changeset when one materialized.
pub(in crate::connection_service) async fn attached_initial_prompt(
    attachments: &AttachmentState<'_>,
    req: &StartSessionRequest,
    os_user: &str,
    sessions_base: &Path,
    session_id: &str,
    progress: &AttachmentProgressSink,
) -> Result<String, Status> {
    let materialized = attachments
        .prepare_session_attachments(&AttachmentMaterialization {
            session_token: &req.session_token,
            os_user,
            sessions_base,
            session_id,
            attachments: &req.attachments,
            progress,
        })
        .await?;
    Ok(
        tddy_session_files::stack_doc_attachments::prompt_with_attached_changeset(
            req.initial_prompt.trim(),
            &materialized,
        ),
    )
}
