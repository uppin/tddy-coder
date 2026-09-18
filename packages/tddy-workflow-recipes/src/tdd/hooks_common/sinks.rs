//! The backend sinks both hook workflows install in `on_enter_task`. The parent `hooks_common`
//! re-exports them, so every `hooks_common::*_sink` call site resolves unchanged.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use tddy_core::backend::{AgentOutputSink, ProgressSink};
use tddy_core::changeset::{read_changeset, SessionEntry};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::stream::ProgressEvent as StreamProgressEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::recipe::WorkflowRecipe;

use super::write_changeset_logged;

/// The agent-output sink both workflows install in `on_enter_task`.
pub(crate) fn agent_output_sink(
    event_tx: Option<&mpsc::Sender<WorkflowEvent>>,
) -> Option<AgentOutputSink> {
    event_tx.map(|tx| {
        let tx = tx.clone();
        AgentOutputSink::new(move |s: &str| {
            let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
        })
    })
}

/// The progress sink both workflows install in `on_enter_task`. `changeset_operation` carries the
/// only difference: the operation each one records on the `SessionStarted` write.
pub(crate) fn progress_sink(
    context: &Context,
    recipe: Arc<dyn WorkflowRecipe>,
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
    changeset_operation: &'static str,
) -> Option<ProgressSink> {
    let session_dir: Option<PathBuf> = context
        .get_sync("session_dir")
        .or_else(|| context.get_sync("output_dir"));
    let task_id: Option<String> = context.get_sync("current_task_id");
    let backend_name: String = context
        .get_sync("backend_name")
        .unwrap_or_else(|| "claude".to_string());

    Some(ProgressSink::new(move |ev: &StreamProgressEvent| {
        if let StreamProgressEvent::SessionStarted { session_id } = ev {
            if let Some(ref dir) = session_dir {
                if let Ok(mut cs) = read_changeset(dir) {
                    let already_exists = cs.sessions.iter().any(|s| s.id == *session_id);
                    if !already_exists {
                        let tag = match task_id.as_deref() {
                            Some("red") => "impl".to_string(),
                            Some(t) => t.to_string(),
                            None => recipe.start_goal().to_string(),
                        };
                        let now = chrono::Utc::now().to_rfc3339();
                        cs.sessions.push(SessionEntry {
                            id: session_id.clone(),
                            agent: backend_name.clone(),
                            tag,
                            created_at: now,
                            system_prompt_file: None,
                        });
                    }
                    cs.state.session_id = Some(session_id.clone());
                    write_changeset_logged(dir, &cs, changeset_operation);
                }
            }
        }
        if let Some(ref tx) = event_tx {
            let _ = tx.send(WorkflowEvent::Progress(ev.clone()));
        }
    }))
}
