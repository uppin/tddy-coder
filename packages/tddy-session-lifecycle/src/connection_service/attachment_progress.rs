use tddy_core::output::SESSIONS_SUBDIR;
use tddy_service::proto::session::start_session_event::Event as StartSessionEventKind;

use std::path::PathBuf;

use tddy_service::proto::session::AttachmentMaterializationProgress;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionEvent;

use tddy_service::proto::session::SessionAttachment;

use std::path::Path;

pub(crate) fn cleanup_materialized_attachments(session_dir: &Path, written: &[SessionAttachment]) {
    let attachments_dir = tddy_workflow::session_attachments_root(session_dir);
    for basename in written.iter().map(|a| &a.basename) {
        let path = attachments_dir.join(basename);
        if path.is_file() {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("cleanup_materialized_attachments: remove {path:?} failed: {e}");
            }
        }
    }
}

/// On-disk size of a just-materialized attachment. An unreadable entry reports 0 rather than
/// failing the start — the bytes are already written, and this value only feeds a progress event.
pub(crate) fn attachment_size_bytes(session_dir: &Path, basename: &str) -> u64 {
    std::fs::metadata(tddy_workflow::session_attachments_root(session_dir).join(basename))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Where attachment-materialization progress goes while a start-session request is being served.
///
/// `StreamStartSession` supplies the stream's sender; unary `StartSession` supplies
/// [`AttachmentProgressSink::discarding`], so the two entry points run the identical code path and
/// the unary one simply has nowhere to report to.
pub(crate) struct AttachmentProgressSink {
    pub(crate) tx: Option<tokio::sync::mpsc::UnboundedSender<Result<StartSessionEvent, Status>>>,
}

impl AttachmentProgressSink {
    /// A sink that reports nowhere — the unary `StartSession` path.
    pub(crate) fn discarding() -> Self {
        Self { tx: None }
    }

    pub(crate) fn streaming(
        tx: tokio::sync::mpsc::UnboundedSender<Result<StartSessionEvent, Status>>,
    ) -> Self {
        Self { tx: Some(tx) }
    }

    /// Reports one attachment's progress. A closed receiver (the client hung up) is ignored: the
    /// session start is already under way and is not abandoned because nobody is watching.
    pub(crate) fn report(&self, progress: AttachmentMaterializationProgress) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        let _ = tx.send(Ok(StartSessionEvent {
            event: Some(StartSessionEventKind::AttachmentProgress(progress)),
        }));
    }
}

/// The attachment currently being materialized, bound to where its progress goes.
///
/// A source whose bytes arrive over time reports through this **as they arrive**, so a row's
/// progress bar moves during the transfer. That is not cosmetic: a forwarded stream terminates a
/// relay that goes [`crate::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`] without a
/// frame, so reporting only once an attachment has fully landed would leave that per-frame deadline
/// covering a whole cross-host transfer.
pub(crate) struct AttachmentProgressReporter<'a> {
    pub(crate) sink: &'a AttachmentProgressSink,
    pub(crate) basename: &'a str,
    pub(crate) attachment_index: u32,
    pub(crate) attachment_count: u32,
}

impl AttachmentProgressReporter<'_> {
    pub(crate) fn report(&self, bytes_done: u64, bytes_total: u64) {
        self.sink.report(AttachmentMaterializationProgress {
            basename: self.basename.to_string(),
            attachment_index: self.attachment_index,
            attachment_count: self.attachment_count,
            bytes_done,
            bytes_total,
        });
    }
}

/// Everything materializing one start-session request's attachments needs: who asked, where the
/// session lives, what to attach, and where progress goes.
///
/// One cohesive context rather than six carried parameters — every field travels together from the
/// per-session-type branch in [`DaemonSessionHost::start_session_core`] down to the copy.
pub(crate) struct AttachmentMaterialization<'a> {
    pub(crate) session_token: &'a str,
    pub(crate) os_user: &'a str,
    pub(crate) sessions_base: &'a Path,
    pub(crate) session_id: &'a str,
    pub(crate) attachments: &'a [SessionAttachment],
    pub(crate) progress: &'a AttachmentProgressSink,
}

impl AttachmentMaterialization<'_> {
    pub(crate) fn session_dir(&self) -> PathBuf {
        self.sessions_base
            .join(SESSIONS_SUBDIR)
            .join(self.session_id)
    }
}
