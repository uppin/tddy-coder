//! Workflow thread runner — runs the full TDD workflow and sends events.
//!
//! Uses WorkflowEngine + FlowRunner with TddWorkflowHooks (event_tx) for TUI integration.

use std::path::PathBuf;
use std::sync::mpsc;

use crate::workflow::graph::ExecutionStatus;
use crate::{
    get_session_for_tag, next_goal_for_state, parse_refactor_response, read_changeset,
    ClarificationQuestion, SharedBackend, WorkflowEngine,
};

use super::WorkflowEvent;

fn demo_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Demo".to_string(),
        question: "Create & run a demo?".to_string(),
        options: vec![
            crate::QuestionOption {
                label: "Create & run".to_string(),
                description: "Create and run the demo script".to_string(),
            },
            crate::QuestionOption {
                label: "Skip".to_string(),
                description: "Skip demo".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Run the full workflow in a blocking thread. Sends events to event_tx, receives answers from answer_rx.
pub fn run_workflow(
    backend: SharedBackend,
    event_tx: mpsc::Sender<WorkflowEvent>,
    answer_rx: mpsc::Receiver<String>,
    output_dir: PathBuf,
    plan_dir: Option<PathBuf>,
    model: Option<String>,
    initial_prompt: Option<String>,
) {
    let inherit_stdin = true;
    let debug = false;

    let plan_dir = match plan_dir {
        Some(p) => p,
        None => {
            let input = match initial_prompt {
                Some(p) => p,
                None => match answer_rx.recv() {
                    Ok(s) => s,
                    Err(_) => return,
                },
            };
            let input = input.trim().to_string();
            if input.is_empty() {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                    "empty feature description".into(),
                )));
                return;
            }
            let plan_dir = output_dir.join(crate::output::slugify_directory_name(&input));
            std::fs::create_dir_all(&plan_dir).ok();
            let storage_dir = std::env::temp_dir().join("tddy-flowrunner-tui-session");
            std::fs::create_dir_all(&storage_dir).ok();
            let hooks = std::sync::Arc::new(
                crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(event_tx.clone()),
            );
            let engine = WorkflowEngine::new(backend.clone(), storage_dir, Some(hooks));
            let mut context_values = std::collections::HashMap::new();
            context_values.insert("feature_input".to_string(), serde_json::json!(input));
            // output_dir = repo root (parent of plan_dir) so agent can discover Cargo.toml, packages/, etc.
            context_values.insert(
                "output_dir".to_string(),
                serde_json::to_value(output_dir.clone()).unwrap(),
            );
            context_values.insert(
                "plan_dir".to_string(),
                serde_json::to_value(plan_dir.clone()).unwrap(),
            );
            context_values.insert(
                "model".to_string(),
                serde_json::to_value(model.clone()).unwrap(),
            );
            context_values.insert("agent_output".to_string(), serde_json::json!(true));
            context_values.insert(
                "inherit_stdin".to_string(),
                serde_json::json!(inherit_stdin),
            );
            context_values.insert("debug".to_string(), serde_json::json!(debug));
            context_values.insert("run_demo".to_string(), serde_json::json!(false));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let mut result = match rt.block_on(engine.run_full_workflow(context_values)) {
                Ok(r) => r,
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            };
            loop {
                match &result.status {
                    ExecutionStatus::Completed | ExecutionStatus::Paused { .. } => break,
                    ExecutionStatus::WaitingForInput { .. } => {
                        let session = match rt.block_on(engine.get_session(&result.session_id)) {
                            Ok(Some(s)) => s,
                            _ => {
                                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                                    "session not found".into(),
                                )));
                                return;
                            }
                        };
                        let questions: Vec<ClarificationQuestion> = session
                            .context
                            .get_sync("pending_questions")
                            .unwrap_or_default();
                        let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                        let answers = match answer_rx.recv() {
                            Ok(a) => a,
                            Err(_) => return,
                        };
                        let mut updates = std::collections::HashMap::new();
                        updates.insert("answers".to_string(), serde_json::json!(answers));
                        if rt
                            .block_on(engine.update_session_context(&result.session_id, updates))
                            .is_err()
                        {
                            return;
                        }
                        result = match rt.block_on(engine.run_session(&result.session_id)) {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = event_tx
                                    .send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                                return;
                            }
                        };
                    }
                    ExecutionStatus::Error(msg) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                        return;
                    }
                }
            }
            plan_dir
        }
    };

    let cs_pre = read_changeset(&plan_dir).ok();
    let plan_needs_completion = cs_pre.as_ref().is_some_and(|c| {
        c.state.current == "Init"
            && (!plan_dir.join("PRD.md").exists() || get_session_for_tag(c, "plan").is_none())
    });
    if plan_needs_completion {
        let input = cs_pre
            .as_ref()
            .and_then(|c| c.initial_prompt.as_deref())
            .unwrap_or("feature")
            .trim()
            .to_string();
        if !input.is_empty() {
            let storage_dir = std::env::temp_dir().join("tddy-flowrunner-tui-session");
            let hooks = std::sync::Arc::new(
                crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(event_tx.clone()),
            );
            let engine = WorkflowEngine::new(backend.clone(), storage_dir, Some(hooks));
            let output_dir = plan_dir
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| plan_dir.clone());
            let mut ctx = std::collections::HashMap::new();
            ctx.insert("feature_input".to_string(), serde_json::json!(input));
            ctx.insert(
                "output_dir".to_string(),
                serde_json::to_value(output_dir).unwrap(),
            );
            ctx.insert(
                "plan_dir".to_string(),
                serde_json::to_value(plan_dir.clone()).unwrap(),
            );
            ctx.insert(
                "model".to_string(),
                serde_json::to_value(model.clone()).unwrap(),
            );
            ctx.insert("agent_output".to_string(), serde_json::json!(true));
            ctx.insert(
                "inherit_stdin".to_string(),
                serde_json::json!(inherit_stdin),
            );
            ctx.insert("debug".to_string(), serde_json::json!(debug));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let mut result = match rt.block_on(engine.run_goal("plan", ctx)) {
                Ok(r) => r,
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            };
            loop {
                match &result.status {
                    ExecutionStatus::Completed | ExecutionStatus::Paused { .. } => break,
                    ExecutionStatus::WaitingForInput { .. } => {
                        let session = match rt.block_on(engine.get_session(&result.session_id)) {
                            Ok(Some(s)) => s,
                            _ => return,
                        };
                        let questions: Vec<ClarificationQuestion> = session
                            .context
                            .get_sync("pending_questions")
                            .unwrap_or_default();
                        let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                        let answers = match answer_rx.recv() {
                            Ok(a) => a,
                            Err(_) => return,
                        };
                        let mut updates = std::collections::HashMap::new();
                        updates.insert("answers".to_string(), serde_json::json!(answers));
                        if rt
                            .block_on(engine.update_session_context(&result.session_id, updates))
                            .is_err()
                        {
                            return;
                        }
                        result = match rt.block_on(engine.run_session(&result.session_id)) {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = event_tx
                                    .send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                                return;
                            }
                        };
                    }
                    ExecutionStatus::Error(msg) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                        return;
                    }
                }
            }
        }
    }

    // Demo question is asked only when we reach GreenComplete (after green), not before.
    let mut run_demo = false;
    let mut demo_asked = false;

    let cs = read_changeset(&plan_dir).ok();
    let start_goal = cs
        .as_ref()
        .and_then(|c| next_goal_for_state(&c.state.current))
        .unwrap_or("plan");

    let storage_dir = std::env::temp_dir().join("tddy-flowrunner-tui-session");
    std::fs::create_dir_all(&storage_dir).ok();
    let hooks = std::sync::Arc::new(crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(
        event_tx.clone(),
    ));
    let engine = WorkflowEngine::new(backend, storage_dir, Some(hooks));

    let mut context_values = std::collections::HashMap::new();
    context_values.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir.clone()).unwrap(),
    );
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir).unwrap(),
    );
    context_values.insert("model".to_string(), serde_json::to_value(model).unwrap());
    context_values.insert("agent_output".to_string(), serde_json::json!(true));
    context_values.insert(
        "inherit_stdin".to_string(),
        serde_json::json!(inherit_stdin),
    );
    context_values.insert("debug".to_string(), serde_json::json!(debug));
    context_values.insert("run_demo".to_string(), serde_json::json!(run_demo));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = if start_goal == "plan" {
        rt.block_on(engine.run_full_workflow(context_values))
    } else {
        rt.block_on(engine.run_workflow_from(start_goal, context_values))
    };
    let mut result = match result {
        Ok(r) => r,
        Err(e) => {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
            return;
        }
    };

    loop {
        match &result.status {
            ExecutionStatus::Completed => {
                let session_opt = rt
                    .block_on(engine.get_session(&result.session_id))
                    .ok()
                    .flatten();
                let output: Option<String> = session_opt
                    .as_ref()
                    .and_then(|s| s.context.get_sync("output"));
                let summary = output
                    .as_ref()
                    .and_then(|o| parse_refactor_response(o).ok())
                    .map(|r| {
                        format!(
                            "Plan dir: {}\nTasks completed: {}\nTests passing: {}",
                            plan_dir.display(),
                            r.tasks_completed,
                            r.tests_passing
                        )
                    })
                    .unwrap_or_else(|| format!("Plan dir: {}", plan_dir.display()));
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Ok(summary)));
                return;
            }
            ExecutionStatus::Paused { .. } => {
                // Ask demo question only when we've reached GreenComplete (not earlier).
                let current_state = read_changeset(&plan_dir)
                    .ok()
                    .map(|c| c.state.current)
                    .unwrap_or_default();
                if current_state == "GreenComplete"
                    && plan_dir.join("demo-plan.md").exists()
                    && !demo_asked
                {
                    let run_demo_asked = event_tx.send(WorkflowEvent::ClarificationNeeded {
                        questions: vec![demo_question()],
                    });
                    if run_demo_asked.is_ok() {
                        demo_asked = true;
                        run_demo = match answer_rx.recv() {
                            Ok(choice) => !choice.eq_ignore_ascii_case("skip"),
                            Err(_) => false,
                        };
                        let mut updates = std::collections::HashMap::new();
                        updates.insert(
                            "run_demo".to_string(),
                            serde_json::json!(run_demo),
                        );
                        let _ = rt.block_on(engine.update_session_context(
                            &result.session_id,
                            updates,
                        ));
                    }
                }
                result = match rt.block_on(engine.run_session(&result.session_id)) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return;
                    }
                };
            }
            ExecutionStatus::WaitingForInput { .. } => {
                let session = match rt.block_on(engine.get_session(&result.session_id)) {
                    Ok(Some(s)) => s,
                    _ => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                            "session not found".into(),
                        )));
                        return;
                    }
                };
                let questions: Vec<ClarificationQuestion> = session
                    .context
                    .get_sync("pending_questions")
                    .unwrap_or_default();
                let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                let answers = match answer_rx.recv() {
                    Ok(a) => a,
                    Err(_) => return,
                };
                let mut updates = std::collections::HashMap::new();
                updates.insert("answers".to_string(), serde_json::json!(answers));
                if rt
                    .block_on(engine.update_session_context(&result.session_id, updates))
                    .is_err()
                {
                    return;
                }
                result = match rt.block_on(engine.run_session(&result.session_id)) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return;
                    }
                };
            }
            ExecutionStatus::Error(msg) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                return;
            }
        }
    }
}
