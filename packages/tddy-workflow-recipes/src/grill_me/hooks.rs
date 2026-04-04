//! Hooks for [`super::GrillMeRecipe`].

use std::error::Error;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::TaskResult;

use super::prompt;

/// Hooks for the grill-me workflow (**Grill** then **Create plan**).
#[derive(Debug)]
pub struct GrillMeWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl GrillMeWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        log::debug!("GrillMeWorkflowHooks::new event_tx={}", event_tx.is_some());
        Self { event_tx }
    }
}

fn compose_create_plan_user_prompt(context: &Context) -> String {
    let mut blocks: Vec<String> = Vec::new();
    if let Some(f) = context
        .get_sync::<String>("feature_input")
        .filter(|s| !s.trim().is_empty())
    {
        blocks.push(format!("## Original request\n\n{f}"));
    }
    if let Some(o) = context
        .get_sync::<String>("output")
        .filter(|s| !s.trim().is_empty())
    {
        blocks.push(format!("## Prior assistant output (Grill phase)\n\n{o}"));
    }
    if let Some(a) = context
        .get_sync::<String>("answers")
        .filter(|s| !s.trim().is_empty())
    {
        blocks.push(format!("## User answers (clarification)\n\n{a}"));
    }
    if blocks.is_empty() {
        "Create the plan brief from session context.".to_string()
    } else {
        blocks.join("\n\n")
    }
}

impl RunnerHooks for GrillMeWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[grill-me hooks] before_task: {}", task_id);

        match task_id {
            "grill" => {
                context.set_sync("system_prompt", prompt::grill_system_prompt());
                if let Some(answers) = context.get_sync::<String>("answers") {
                    if !answers.trim().is_empty() {
                        log::debug!(
                            "[grill-me hooks] transferring answers to prompt (len={})",
                            answers.len()
                        );
                        context.set_sync("prompt", &answers);
                        context.remove_sync("answers");
                    }
                }
            }
            "create-plan" => {
                let session_dir = context
                    .get_sync::<std::path::PathBuf>("session_dir")
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(session_dir unset)".to_string());
                let output_dir = context
                    .get_sync::<std::path::PathBuf>("output_dir")
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(output_dir unset)".to_string());
                context.set_sync(
                    "system_prompt",
                    prompt::create_plan_system_prompt(&session_dir, &output_dir),
                );
                let composed = compose_create_plan_user_prompt(context);
                context.set_sync("prompt", composed);
                context.remove_sync("answers");
            }
            _ => {}
        }

        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
            let _ = tx.send(WorkflowEvent::StateChange {
                from: String::new(),
                to: task_id.to_string(),
            });
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Relay `tddy-tools ask` answers into context (socket path does not use WaitForInput).
        if task_id == "grill" {
            if let Some(dir) = context.get_sync::<std::path::PathBuf>("session_dir") {
                let path = dir.join(".workflow").join("grill_ask_answers.txt");
                if path.exists() {
                    match std::fs::read_to_string(&path) {
                        Ok(s) => {
                            let t = s.trim();
                            if !t.is_empty() {
                                context.set_sync("answers", t.to_string());
                                log::debug!(
                                    "[grill-me hooks] loaded grill_ask_answers.txt ({} chars)",
                                    t.len()
                                );
                            }
                            let _ = std::fs::remove_file(&path);
                        }
                        Err(e) => {
                            log::warn!("[grill-me hooks] read {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn on_error(&self, _task_id: &str, _context: &Context, _error: &(dyn Error + Send + Sync)) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tddy_core::workflow::context::Context;
    use tddy_core::workflow::task::NextAction;

    #[test]
    fn grill_after_task_loads_grill_ask_answers_into_context() {
        let tmp =
            std::env::temp_dir().join(format!("grill-ask-answers-hook-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".workflow")).unwrap();
        fs::write(
            tmp.join(".workflow").join("grill_ask_answers.txt"),
            "line1\nline2",
        )
        .unwrap();

        let ctx = Context::new();
        ctx.set_sync("session_dir", tmp.clone());

        let hooks = GrillMeWorkflowHooks::new(None);
        let result = TaskResult {
            response: String::new(),
            next_action: NextAction::Continue,
            task_id: "grill".to_string(),
            status_message: None,
        };
        hooks.after_task("grill", &ctx, &result).unwrap();

        assert_eq!(
            ctx.get_sync::<String>("answers").as_deref(),
            Some("line1\nline2")
        );
        assert!(
            !tmp.join(".workflow").join("grill_ask_answers.txt").exists(),
            "handoff file should be removed after load"
        );
        let _ = fs::remove_dir_all(&tmp);
    }
}
