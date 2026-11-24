//! A session's record follows the model its agent reports it is on (`coding_agent.model_report`):
//! a hook report is run through the session's agent spec's sources, and a model they yield that
//! differs from `.session.yaml` `model` replaces it. Every resume relaunches the agent with that
//! field, so a model its operator picked inside the agent (`/model`) survives a resume.
//!
//! A `usage` source evaluates the session's usage binding (`tddy-usage`) over its main
//! conversation's transcript, with no fallback model: a transcript that names none yields none,
//! rather than the start's model undoing a later report.

use std::path::{Path, PathBuf};

use tddy_coding_agent_specs::ModelReportSource;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_rpc::Status;
use tddy_service::proto::activity::ReportSessionStatusRequest;
use tddy_usage::{account, read_usage_context, UsageContext};

use crate::service::ports::UsageBindings;
use crate::service::ActivityServiceImpl;

impl ActivityServiceImpl {
    /// Run `req` through the session's `model_report` sources and record the model they report.
    pub(super) async fn follow_reported_model(
        &self,
        session_dir: &Path,
        req: &ReportSessionStatusRequest,
    ) -> Result<(), Status> {
        let meta = tddy_core::read_session_metadata(session_dir)
            .map_err(|e| Status::internal(format!("failed to read the session record: {e}")))?;
        let session_type = meta.session_type.as_deref().unwrap_or("");
        let Some(spec) = self.agents.for_session_type(session_type) else {
            return Ok(());
        };
        let reported = self
            .reported_model(spec.model_report(), session_dir, &meta, req)
            .await;
        match reported {
            Some(model) => record_model(session_dir, model),
            None => Ok(()),
        }
    }

    /// The model the first of `sources` that applies to `req`'s event yields, if any.
    async fn reported_model(
        &self,
        sources: &[ModelReportSource],
        session_dir: &Path,
        meta: &SessionMetadata,
        req: &ReportSessionStatusRequest,
    ) -> Option<String> {
        for source in sources
            .iter()
            .filter(|source| source.applies_to(&req.hook_event_name))
        {
            let value = match source {
                ModelReportSource::HookField { field, .. } => req.hook_fields.get(field).cloned(),
                ModelReportSource::Usage { .. } => {
                    let bindings = self.ports.usage_bindings.clone();
                    transcript_model(session_dir.to_path_buf(), meta, bindings).await
                }
            };
            if let Some(model) = value.as_deref().and_then(|value| source.accepts(value)) {
                return Some(model.to_string());
            }
        }
        None
    }
}

/// The model the session's usage binding reads last off its current main conversation, or `None`
/// when it has no binding, no transcript, or a transcript naming none.
async fn transcript_model(
    session_dir: PathBuf,
    meta: &SessionMetadata,
    bindings: UsageBindings,
) -> Option<String> {
    let current_conversation = meta.agent_conversations.last().map(|c| c.id.clone());
    let read = tokio::task::spawn_blocking(move || {
        let context = read_usage_context(&session_dir)?;
        let section = bindings(&context.spec_id)?;
        let ctx = UsageContext {
            agent_home: context.agent_home,
            session_dir,
            conversation_id: current_conversation.or(context.conversation_id),
            worktree: context.worktree,
            agent_label: context.spec_id,
            fallback_model: String::new(),
        };
        account(&section, &ctx)
            .into_iter()
            .next()
            .map(|main| main.model)
    })
    .await;
    match read {
        Ok(model) => model,
        Err(e) => {
            log::warn!("model report: reading the transcript failed: {e}");
            None
        }
    }
}

/// Write `model` into the session's record when it differs from the one there. The record is read
/// again here, after any await above, so a status or rotation written meanwhile is kept.
fn record_model(session_dir: &Path, model: String) -> Result<(), Status> {
    let meta = tddy_core::read_session_metadata(session_dir)
        .map_err(|e| Status::internal(format!("failed to read the session record: {e}")))?;
    if meta.model.as_deref() == Some(model.as_str()) {
        return Ok(());
    }
    log::info!(
        "session {}: the agent reports model {model:?} (was {:?})",
        meta.session_id,
        meta.model
    );
    let updated = SessionMetadata {
        model: Some(model),
        ..meta
    };
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to record the reported model: {e}")))
}
