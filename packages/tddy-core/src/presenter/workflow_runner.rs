//! Workflow thread runner — runs the full TDD workflow and sends events.
//!
//! Uses WorkflowEngine + FlowRunner with TddWorkflowHooks (event_tx) for TUI integration.

use std::path::{Path, PathBuf};

/// Directory name for TUI workflow session storage under temp.
const TUI_SESSION_DIR: &str = "tddy-flowrunner-tui-session";
use std::sync::mpsc;

use crate::workflow::graph::{ElicitationEvent, ExecutionResult, ExecutionStatus};
use crate::{
    get_session_for_tag, next_goal_for_state, parse_refactor_response, parse_update_docs_response,
    read_changeset, ClarificationQuestion, SharedBackend, WorkflowEngine,
};

use super::{WorkflowCompletePayload, WorkflowEvent};

/// Context for elicitation (plan approval, refinement). Groups parameters passed to handle_elicitation.
struct ElicitationContext<'a> {
    event_tx: &'a mpsc::Sender<WorkflowEvent>,
    answer_rx: &'a mpsc::Receiver<String>,
    rt: &'a tokio::runtime::Runtime,
    backend: &'a SharedBackend,
    model: &'a Option<String>,
    inherit_stdin: bool,
    conversation_output_path: &'a Option<PathBuf>,
    debug: bool,
}

/// Loop on WaitingForInput until status is Completed, Paused, or ElicitationNeeded.
/// Returns Ok(result) or Err(()) if the caller should return.
fn run_until_not_waiting_for_input(
    rt: &tokio::runtime::Runtime,
    engine: &WorkflowEngine,
    mut result: ExecutionResult,
    event_tx: &mpsc::Sender<WorkflowEvent>,
    answer_rx: &mpsc::Receiver<String>,
) -> Result<ExecutionResult, ()> {
    loop {
        match &result.status {
            ExecutionStatus::Completed
            | ExecutionStatus::Paused { .. }
            | ExecutionStatus::ElicitationNeeded { .. } => return Ok(result),
            ExecutionStatus::WaitingForInput { .. } => {
                result = handle_clarification_round(rt, engine, &result, event_tx, answer_rx)?;
            }
            ExecutionStatus::Error(msg) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                return Err(());
            }
        }
    }
}

/// Handle one round of WaitingForInput: get session, send questions, receive answers,
/// update context, run session. Returns the new result or Err(()) if the caller should return.
fn handle_clarification_round(
    rt: &tokio::runtime::Runtime,
    engine: &WorkflowEngine,
    result: &ExecutionResult,
    event_tx: &mpsc::Sender<WorkflowEvent>,
    answer_rx: &mpsc::Receiver<String>,
) -> Result<ExecutionResult, ()> {
    let session = match rt.block_on(engine.get_session(&result.session_id)) {
        Ok(Some(s)) => s,
        _ => {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                "session not found".into()
            )));
            return Err(());
        }
    };
    let questions: Vec<ClarificationQuestion> = session
        .context
        .get_sync("pending_questions")
        .unwrap_or_default();
    let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
    let answers = match answer_rx.recv() {
        Ok(a) => a,
        Err(_) => return Err(()),
    };
    let mut updates = std::collections::HashMap::new();
    updates.insert("answers".to_string(), serde_json::json!(answers));
    if rt
        .block_on(engine.update_session_context(&result.session_id, updates))
        .is_err()
    {
        return Err(());
    }
    match rt.block_on(engine.run_session(&result.session_id)) {
        Ok(r) => Ok(r),
        Err(e) => {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
            Err(())
        }
    }
}

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

/// Handle an ElicitationNeeded result: show approval UI, handle refinement loop.
/// Returns once the user approves the plan.
fn handle_elicitation(
    event: &ElicitationEvent,
    plan_dir: &Path,
    ctx: &ElicitationContext<'_>,
) -> bool {
    match event {
        ElicitationEvent::PlanApproval { ref prd_content } => {
            let mut current_prd = prd_content.clone();
            loop {
                if ctx
                    .event_tx
                    .send(WorkflowEvent::PlanApprovalNeeded {
                        prd_content: current_prd.clone(),
                    })
                    .is_err()
                {
                    return false;
                }
                let answer = match ctx.answer_rx.recv() {
                    Ok(a) => a,
                    Err(_) => return false,
                };
                if answer.eq_ignore_ascii_case("approve") {
                    return true;
                }
                let feature_input = read_changeset(plan_dir)
                    .ok()
                    .and_then(|c| c.initial_prompt.clone())
                    .unwrap_or_else(|| "feature".to_string());
                let session_id_for_refine = read_changeset(plan_dir)
                    .ok()
                    .and_then(|c| get_session_for_tag(&c, "plan"));
                let output_dir_refine = plan_dir
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| plan_dir.to_path_buf());
                let refine_storage = std::env::temp_dir().join("tddy-flowrunner-refine-session");
                std::fs::create_dir_all(&refine_storage).ok();
                let refine_hooks = std::sync::Arc::new(
                    crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(
                        ctx.event_tx.clone(),
                    ),
                );
                let refine_engine =
                    WorkflowEngine::new(ctx.backend.clone(), refine_storage, Some(refine_hooks));
                let mut refine_ctx = std::collections::HashMap::new();
                refine_ctx.insert(
                    "feature_input".to_string(),
                    serde_json::json!(feature_input),
                );
                refine_ctx.insert(
                    "output_dir".to_string(),
                    serde_json::to_value(&output_dir_refine).unwrap(),
                );
                refine_ctx.insert(
                    "plan_dir".to_string(),
                    serde_json::to_value(plan_dir).unwrap(),
                );
                refine_ctx.insert("refinement_feedback".to_string(), serde_json::json!(answer));
                refine_ctx.insert(
                    "model".to_string(),
                    serde_json::to_value(ctx.model.clone().unwrap_or_default()).unwrap(),
                );
                refine_ctx.insert("agent_output".to_string(), serde_json::json!(true));
                refine_ctx.insert(
                    "inherit_stdin".to_string(),
                    serde_json::json!(ctx.inherit_stdin),
                );
                refine_ctx.insert(
                    "conversation_output_path".to_string(),
                    serde_json::to_value(ctx.conversation_output_path.clone()).unwrap(),
                );
                refine_ctx.insert("debug".to_string(), serde_json::json!(ctx.debug));
                if let Some(sid) = session_id_for_refine {
                    refine_ctx.insert("session_id".to_string(), serde_json::json!(sid));
                }
                let mut refine_result =
                    match ctx.rt.block_on(refine_engine.run_goal("plan", refine_ctx)) {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = ctx
                                .event_tx
                                .send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                            return false;
                        }
                    };
                loop {
                    match &refine_result.status {
                        ExecutionStatus::Completed
                        | ExecutionStatus::Paused { .. }
                        | ExecutionStatus::ElicitationNeeded { .. } => break,
                        ExecutionStatus::WaitingForInput { .. } => {
                            refine_result = match handle_clarification_round(
                                ctx.rt,
                                &refine_engine,
                                &refine_result,
                                ctx.event_tx,
                                ctx.answer_rx,
                            ) {
                                Ok(r) => r,
                                Err(()) => return false,
                            };
                        }
                        ExecutionStatus::Error(msg) => {
                            let _ = ctx
                                .event_tx
                                .send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                            return false;
                        }
                    }
                }
                current_prd = std::fs::read_to_string(plan_dir.join("PRD.md"))
                    .unwrap_or_else(|_| "Could not read PRD.md".to_string());
            }
        }
    }
}

/// Run plan goal when output_dir is omitted (or "."). Creates session under ~/.tddy/sessions.
/// Returns Some(plan_dir) on success, None when the caller should return.
#[allow(clippy::too_many_arguments)]
fn run_plan_without_output_dir(
    backend: &SharedBackend,
    event_tx: &mpsc::Sender<WorkflowEvent>,
    answer_rx: &mpsc::Receiver<String>,
    output_dir: &Path,
    input: &str,
    session_id: Option<&str>,
    model: &Option<String>,
    conversation_output_path: &Option<PathBuf>,
    debug_output_path: Option<&Path>,
    debug: bool,
) -> Option<PathBuf> {
    let inherit_stdin = true;
    let (output_dir_for_ctx, session_base_opt) = if output_dir == Path::new(".") {
        #[cfg(unix)]
        {
            let home = match std::env::var("HOME") {
                Ok(h) => PathBuf::from(h).join(".tddy"),
                Err(_) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                        "HOME not set; cannot create session under ~/.tddy".into(),
                    )));
                    return None;
                }
            };
            let agent_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            (agent_cwd, Some(home))
        }
        #[cfg(not(unix))]
        {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                "plan without --output-dir requires Unix (HOME); use --output-dir <path>".into(),
            )));
            return None;
        }
    } else if session_id.is_some() {
        (output_dir.to_path_buf(), Some(output_dir.to_path_buf()))
    } else {
        (output_dir.to_path_buf(), None)
    };

    let storage_dir = std::env::temp_dir().join(TUI_SESSION_DIR);
    std::fs::create_dir_all(&storage_dir).ok();
    let hooks = std::sync::Arc::new(crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(
        event_tx.clone(),
    ));
    let engine = WorkflowEngine::new(backend.clone(), storage_dir, Some(hooks));
    let mut context_values = std::collections::HashMap::new();
    context_values.insert("feature_input".to_string(), serde_json::json!(input));
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir_for_ctx).unwrap(),
    );
    if let Some(ref base) = session_base_opt {
        context_values.insert(
            "session_base".to_string(),
            serde_json::to_value(base).unwrap(),
        );
    }
    if let Some(sid) = session_id {
        context_values.insert("session_id".to_string(), serde_json::json!(sid));
    }
    context_values.insert(
        "model".to_string(),
        serde_json::to_value(model.clone()).unwrap(),
    );
    context_values.insert("agent_output".to_string(), serde_json::json!(true));
    context_values.insert(
        "inherit_stdin".to_string(),
        serde_json::json!(inherit_stdin),
    );
    context_values.insert(
        "conversation_output_path".to_string(),
        serde_json::to_value(conversation_output_path.clone()).unwrap(),
    );
    context_values.insert("debug".to_string(), serde_json::json!(debug));
    context_values.insert("run_demo".to_string(), serde_json::json!(false));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = match rt.block_on(engine.run_full_workflow(context_values)) {
        Ok(r) => r,
        Err(e) => {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
            return None;
        }
    };

    let result = match run_until_not_waiting_for_input(&rt, &engine, result, event_tx, answer_rx) {
        Ok(r) => r,
        Err(()) => return None,
    };

    let plan_dir = rt
        .block_on(engine.get_session(&result.session_id))
        .ok()
        .flatten()
        .and_then(|s| s.context.get_sync::<PathBuf>("plan_dir"))
        .unwrap_or_else(|| output_dir.join(crate::output::slugify_directory_name(input)));

    let conversation_output_resolved = crate::resolve_log_defaults(
        conversation_output_path.clone(),
        debug_output_path,
        &plan_dir,
    );

    if let ExecutionStatus::ElicitationNeeded { ref event } = result.status {
        let elicitation_ctx = ElicitationContext {
            event_tx,
            answer_rx,
            rt: &rt,
            backend,
            model,
            inherit_stdin,
            conversation_output_path: &conversation_output_resolved,
            debug,
        };
        if !handle_elicitation(event, &plan_dir, &elicitation_ctx) {
            return None;
        }
    }

    Some(plan_dir)
}

/// Run the full workflow in a blocking thread. Sends events to event_tx, receives answers from answer_rx.
#[allow(clippy::too_many_arguments)]
pub fn run_workflow(
    backend: SharedBackend,
    event_tx: mpsc::Sender<WorkflowEvent>,
    answer_rx: mpsc::Receiver<String>,
    output_dir: PathBuf,
    plan_dir: Option<PathBuf>,
    session_id: Option<String>,
    model: Option<String>,
    initial_prompt: Option<String>,
    conversation_output_path: Option<PathBuf>,
    debug_output_path: Option<PathBuf>,
    debug: bool,
) {
    let inherit_stdin = true;

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
            match run_plan_without_output_dir(
                &backend,
                &event_tx,
                &answer_rx,
                &output_dir,
                &input,
                session_id.as_deref(),
                &model,
                &conversation_output_path,
                debug_output_path.as_deref(),
                debug,
            ) {
                Some(p) => p,
                None => return,
            }
        }
    };

    let conversation_output_path =
        crate::resolve_log_defaults(conversation_output_path, debug_output_path, &plan_dir);

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
            let storage_dir = std::env::temp_dir().join(TUI_SESSION_DIR);
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
            ctx.insert(
                "conversation_output_path".to_string(),
                serde_json::to_value(conversation_output_path.clone()).unwrap(),
            );
            ctx.insert("debug".to_string(), serde_json::json!(debug));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let result = match rt.block_on(engine.run_goal("plan", ctx)) {
                Ok(r) => r,
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            };
            let result = match run_until_not_waiting_for_input(
                &rt, &engine, result, &event_tx, &answer_rx,
            ) {
                Ok(r) => r,
                Err(()) => return,
            };
            if let ExecutionStatus::ElicitationNeeded { ref event } = result.status {
                let elicitation_ctx = ElicitationContext {
                    event_tx: &event_tx,
                    answer_rx: &answer_rx,
                    rt: &rt,
                    backend: &backend,
                    model: &model,
                    inherit_stdin,
                    conversation_output_path: &conversation_output_path,
                    debug,
                };
                if !handle_elicitation(event, &plan_dir, &elicitation_ctx) {
                    return;
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

    let storage_dir = std::env::temp_dir().join(TUI_SESSION_DIR);
    std::fs::create_dir_all(&storage_dir).ok();
    let hooks = std::sync::Arc::new(crate::workflow::tdd_hooks::TddWorkflowHooks::with_event_tx(
        event_tx.clone(),
    ));
    let backend_for_refine = backend.clone();
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
    context_values.insert(
        "model".to_string(),
        serde_json::to_value(model.clone()).unwrap(),
    );
    context_values.insert("agent_output".to_string(), serde_json::json!(true));
    context_values.insert(
        "inherit_stdin".to_string(),
        serde_json::json!(inherit_stdin),
    );
    context_values.insert(
        "conversation_output_path".to_string(),
        serde_json::to_value(conversation_output_path.clone()).unwrap(),
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
                    .and_then(|o| {
                        parse_update_docs_response(o)
                            .ok()
                            .map(|r| {
                                format!(
                                    "Plan dir: {}\nDocs updated: {}",
                                    plan_dir.display(),
                                    r.docs_updated
                                )
                            })
                            .or_else(|| {
                                parse_refactor_response(o).ok().map(|r| {
                                    format!(
                                        "Plan dir: {}\nTasks completed: {}\nTests passing: {}",
                                        plan_dir.display(),
                                        r.tasks_completed,
                                        r.tests_passing
                                    )
                                })
                            })
                    })
                    .unwrap_or_else(|| format!("Plan dir: {}", plan_dir.display()));
                let payload = WorkflowCompletePayload {
                    summary,
                    plan_dir: Some(plan_dir.clone()),
                };
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Ok(payload)));
                return;
            }
            ExecutionStatus::ElicitationNeeded { ref event } => {
                let elicitation_ctx = ElicitationContext {
                    event_tx: &event_tx,
                    answer_rx: &answer_rx,
                    rt: &rt,
                    backend: &backend_for_refine,
                    model: &model,
                    inherit_stdin,
                    conversation_output_path: &conversation_output_path,
                    debug,
                };
                if !handle_elicitation(event, &plan_dir, &elicitation_ctx) {
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
            ExecutionStatus::Paused { .. } => {
                let current_state = read_changeset(&plan_dir)
                    .ok()
                    .map(|c| c.state.current)
                    .unwrap_or_default();

                // Ask demo question only when we've reached GreenComplete (not earlier).
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
                        updates.insert("run_demo".to_string(), serde_json::json!(run_demo));
                        let _ =
                            rt.block_on(engine.update_session_context(&result.session_id, updates));
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
                result = match handle_clarification_round(
                    &rt, &engine, &result, &event_tx, &answer_rx,
                ) {
                    Ok(r) => r,
                    Err(()) => return,
                };
            }
            ExecutionStatus::Error(msg) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(msg.clone())));
                return;
            }
        }
    }
}
