//! Workflow thread runner — runs the full TDD workflow and sends events.
//!
//! Uses WorkflowEngine + FlowRunner with recipe-provided hooks (event_tx) for TUI integration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fallback when plan has not created `session_dir` yet (rare); prefer [`crate::workflow::session::workflow_engine_storage_dir`].
const TUI_SESSION_FALLBACK_DIR: &str = "tddy-flowrunner-tui-session";
use std::sync::mpsc;

use crate::backend::WorkflowRecipe;
use crate::workflow::graph::{ElicitationEvent, ExecutionResult, ExecutionStatus};
use crate::{
    get_session_for_tag, read_changeset, ClarificationQuestion, SharedBackend, WorkflowEngine,
};

use super::{WorkflowCompletePayload, WorkflowEvent};

/// Context for elicitation (plan approval, refinement). Groups parameters passed to handle_elicitation.
struct ElicitationContext<'a> {
    recipe: &'a Arc<dyn WorkflowRecipe>,
    event_tx: &'a mpsc::Sender<WorkflowEvent>,
    answer_rx: &'a mpsc::Receiver<String>,
    rt: &'a tokio::runtime::Runtime,
    backend: &'a SharedBackend,
    model: &'a Option<String>,
    inherit_stdin: bool,
    conversation_output_path: &'a Option<PathBuf>,
    debug: bool,
    socket_path: Option<&'a PathBuf>,
}

/// Model, agent I/O, logging, optional `run_demo`, and optional socket — shared across plan/start/refine contexts.
fn insert_engine_io_and_demo_flags(
    ctx: &mut std::collections::HashMap<String, serde_json::Value>,
    model: &Option<String>,
    inherit_stdin: bool,
    conversation_output_path: &Option<PathBuf>,
    debug: bool,
    run_demo: Option<bool>,
    socket_path: Option<&PathBuf>,
) {
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
    if let Some(rd) = run_demo {
        ctx.insert("run_demo".to_string(), serde_json::json!(rd));
    }
    if let Some(p) = socket_path {
        ctx.insert("socket_path".to_string(), serde_json::to_value(p).unwrap());
    }
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

/// Handle an ElicitationNeeded result: show approval UI, handle refinement loop.
/// Returns once the user approves the plan.
fn handle_elicitation(
    event: &ElicitationEvent,
    session_dir: &Path,
    ctx: &ElicitationContext<'_>,
) -> bool {
    match event {
        ElicitationEvent::DocumentApproval { ref content } => {
            let mut current_prd = content.clone();
            loop {
                if ctx
                    .event_tx
                    .send(WorkflowEvent::SessionDocumentApprovalNeeded {
                        content: current_prd.clone(),
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
                if answer.eq_ignore_ascii_case("reject") {
                    let _ = ctx.event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                        "Session document rejected by operator".to_string(),
                    )));
                    return false;
                }
                let feature_input = read_changeset(session_dir)
                    .ok()
                    .and_then(|c| c.initial_prompt.clone())
                    .unwrap_or_else(|| "feature".to_string());
                let session_id_for_refine = read_changeset(session_dir)
                    .ok()
                    .and_then(|c| get_session_for_tag(&c, ctx.recipe.start_goal().as_str()));
                let output_dir_refine = read_changeset(session_dir)
                    .ok()
                    .and_then(|c| c.repo_path.clone())
                    .map(PathBuf::from)
                    .or_else(|| session_dir.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| session_dir.to_path_buf());
                let refine_storage =
                    crate::workflow::session::workflow_engine_storage_dir(session_dir);
                std::fs::create_dir_all(&refine_storage).ok();
                let refine_hooks = ctx.recipe.create_hooks(Some(ctx.event_tx.clone()));
                let refine_engine = WorkflowEngine::new(
                    ctx.recipe.clone(),
                    ctx.backend.clone(),
                    refine_storage,
                    Some(refine_hooks),
                );
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
                    "session_dir".to_string(),
                    serde_json::to_value(session_dir).unwrap(),
                );
                refine_ctx.insert("refinement_feedback".to_string(), serde_json::json!(answer));
                let refine_model = Some(ctx.model.clone().unwrap_or_default());
                insert_engine_io_and_demo_flags(
                    &mut refine_ctx,
                    &refine_model,
                    ctx.inherit_stdin,
                    ctx.conversation_output_path,
                    ctx.debug,
                    None,
                    ctx.socket_path,
                );
                if let Some(sid) = session_id_for_refine {
                    refine_ctx.insert("session_id".to_string(), serde_json::json!(sid));
                }
                let start_goal = ctx.recipe.start_goal().clone();
                let mut refine_result = match ctx
                    .rt
                    .block_on(refine_engine.run_goal(&start_goal, refine_ctx))
                {
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
                current_prd = ctx
                    .recipe
                    .read_primary_session_document_utf8(session_dir)
                    .unwrap_or_else(|| current_prd.clone());
            }
        }
        ElicitationEvent::WorktreeConfirmation { .. } => {
            // WorktreeConfirmation is only used in daemon mode; handled by DaemonService.
            let _ = ctx.event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                "WorktreeConfirmation not supported in TUI mode".to_string(),
            )));
            false
        }
    }
}

/// Run plan goal when output_dir is omitted (or "."). Creates session under ~/.tddy/sessions.
/// Returns Some(session_dir) on success, None when the caller should return.
#[allow(clippy::too_many_arguments)]
fn run_start_goal_without_output_dir(
    recipe: Arc<dyn WorkflowRecipe>,
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
    socket_path: Option<&PathBuf>,
) -> Option<PathBuf> {
    let inherit_stdin = false;
    let (output_dir_for_ctx, session_base_opt) = if output_dir == Path::new(".") {
        match crate::output::tddy_data_dir_path() {
            Ok(base) => {
                let agent_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                (agent_cwd, Some(base))
            }
            Err(e) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(format!("{}", e))));
                return None;
            }
        }
    } else if session_id.is_some() {
        (output_dir.to_path_buf(), Some(output_dir.to_path_buf()))
    } else {
        (output_dir.to_path_buf(), None)
    };

    let repo_path_str = output_dir_for_ctx.display().to_string();
    let mut context_values = std::collections::HashMap::new();
    context_values.insert("feature_input".to_string(), serde_json::json!(input));
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(&output_dir_for_ctx).unwrap(),
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
    let mut session_dir: Option<PathBuf> = match (&session_base_opt, session_id) {
        (Some(base), Some(sid)) => Some(
            crate::output::create_session_dir_with_id(base, sid)
                .unwrap_or_else(|_| base.join(crate::output::SESSIONS_SUBDIR).join(sid)),
        ),
        _ => None,
    };

    // Recipes whose first task does not run TDD `before_plan` (e.g. bugfix `reproduce` echo) never
    // call `new_session_dir()`; the engine session would then lack `session_dir`. Allocate under
    // `session_base_opt` or `tddy_data_dir_path()` so context and disk layout match TDD entry.
    if session_dir.is_none() {
        let base = match session_base_opt.clone() {
            Some(b) => b,
            None => match crate::output::tddy_data_dir_path() {
                Ok(b) => b,
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(format!("{}", e))));
                    return None;
                }
            },
        };
        if !context_values.contains_key("session_base") {
            context_values.insert(
                "session_base".to_string(),
                serde_json::to_value(&base).unwrap(),
            );
        }
        match crate::output::create_session_dir_in(&base) {
            Ok(dir) => {
                let Some(sid) = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                else {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                        "allocated session directory has no basename".into(),
                    )));
                    return None;
                };
                context_values.insert("session_id".to_string(), serde_json::json!(sid));
                session_dir = Some(dir);
            }
            Err(e) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                return None;
            }
        }
    }

    let storage_dir = match session_dir.as_ref() {
        Some(dir) => crate::workflow::session::workflow_engine_storage_dir(dir),
        None => std::env::temp_dir().join(TUI_SESSION_FALLBACK_DIR),
    };
    std::fs::create_dir_all(&storage_dir).ok();
    let hooks = recipe.create_hooks(Some(event_tx.clone()));
    let engine = WorkflowEngine::new(recipe.clone(), backend.clone(), storage_dir, Some(hooks));
    if let Some(ref dir) = session_dir {
        let recipe_name = recipe.name().to_string();
        let init_cs = crate::changeset::Changeset {
            initial_prompt: Some(input.to_string()),
            repo_path: Some(repo_path_str.clone()),
            recipe: Some(recipe_name.clone()),
            ..crate::changeset::Changeset::default()
        };
        log::info!(
            "[workflow_runner] run_start_goal_without_output_dir: wrote changeset.yaml recipe={}",
            recipe_name
        );
        if let Err(e) = crate::changeset::write_changeset(dir, &init_cs) {
            log::warn!("write_changeset (run_start_goal_without_output_dir): {}", e);
        }
        if let Err(e) = crate::session_metadata::write_initial_tool_session_metadata(
            dir,
            crate::session_metadata::InitialToolSessionMetadataOpts {
                project_id: String::new(),
                repo_path: Some(repo_path_str),
                pid: Some(std::process::id()),
                tool: Some("tddy-coder".to_string()),
                livekit_room: None,
                previous_session_id: None,
            },
        ) {
            log::warn!("write_initial_tool_session_metadata: {}", e);
        }
        context_values.insert(
            "session_dir".to_string(),
            serde_json::to_value(dir).unwrap(),
        );
    }
    if debug_output_path.is_none() {
        if let Some(ref dir) = session_dir {
            let logs = dir.join("logs");
            let _ = std::fs::create_dir_all(&logs);
            crate::redirect_debug_output(&logs.join("debug.log"));
            log::set_max_level(log::LevelFilter::Debug);
        }
    }
    let (temp_conv_path, session_conv_path) = if conversation_output_path.is_none() {
        if let Some(ref dir) = session_dir {
            let conv_path = dir.join("logs").join("conversation.jsonl");
            if let Some(parent) = conv_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            (None, Some(conv_path))
        } else {
            let p =
                std::env::temp_dir().join(format!("tddy-plan-conv-{}.jsonl", std::process::id()));
            let _ = std::fs::remove_file(&p);
            (Some(p), None)
        }
    } else {
        (None, None)
    };
    let conv_for_ctx = conversation_output_path
        .clone()
        .or(session_conv_path)
        .or_else(|| temp_conv_path.clone());
    insert_engine_io_and_demo_flags(
        &mut context_values,
        model,
        inherit_stdin,
        &conv_for_ctx,
        debug,
        Some(false),
        socket_path,
    );
    context_values.insert("run_optional_step_x".to_string(), serde_json::json!(false));

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

    let session_dir = match rt
        .block_on(engine.get_session(&result.session_id))
        .ok()
        .flatten()
    {
        Some(s) => {
            if let Some(p) = s.context.get_sync::<PathBuf>("session_dir") {
                log::debug!(
                    "workflow_runner: using session_dir from engine context {:?}",
                    p
                );
                p
            } else if let (Some(base), Some(sid)) = (
                s.context.get_sync::<PathBuf>("session_base"),
                s.context.get_sync::<String>("session_id"),
            ) {
                log::info!(
                    "workflow_runner: session_dir missing; materializing from session_base {:?} and session_id",
                    base
                );
                match crate::output::create_session_dir_with_id(&base, &sid) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return None;
                    }
                }
            } else {
                log::error!(
                    "workflow_runner: missing session_dir and session_base+session_id in engine context after workflow"
                );
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                    "session directory could not be resolved: workflow context missing session_dir and session_base/session_id"
                        .into(),
                )));
                return None;
            }
        }
        None => {
            let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                "session not found after workflow".into(),
            )));
            return None;
        }
    };

    let conversation_output_resolved = crate::resolve_log_defaults(
        conversation_output_path.clone(),
        debug_output_path,
        &session_dir,
    );

    if let Some(ref temp) = temp_conv_path {
        if temp.exists() {
            if let Some(ref final_path) = conversation_output_resolved {
                if let Some(parent) = final_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(temp, final_path)
                    .or_else(|_| std::fs::copy(temp, final_path).map(|_| ()));
                let _ = std::fs::remove_file(temp);
            }
        }
    }

    if let ExecutionStatus::ElicitationNeeded { ref event } = result.status {
        let elicitation_ctx = ElicitationContext {
            recipe: &recipe,
            event_tx,
            answer_rx,
            rt: &rt,
            backend,
            model,
            inherit_stdin,
            conversation_output_path: &conversation_output_resolved,
            debug,
            socket_path,
        };
        if !handle_elicitation(event, &session_dir, &elicitation_ctx) {
            return None;
        }
    }

    Some(session_dir)
}

/// Run the full workflow in a blocking thread. Sends events to event_tx, receives answers from answer_rx.
#[allow(clippy::too_many_arguments)]
pub fn run_workflow(
    recipe: Arc<dyn WorkflowRecipe>,
    backend: SharedBackend,
    event_tx: mpsc::Sender<WorkflowEvent>,
    answer_rx: mpsc::Receiver<String>,
    output_dir: PathBuf,
    session_dir: Option<PathBuf>,
    session_id: Option<String>,
    model: Option<String>,
    initial_prompt: Option<String>,
    conversation_output_path: Option<PathBuf>,
    debug_output_path: Option<PathBuf>,
    debug: bool,
    socket_path: Option<PathBuf>,
    worktree_dir: Option<PathBuf>,
) {
    let inherit_stdin = false;
    let initial_prompt_for_ctx = initial_prompt.clone();

    let output_dir_was_dot = output_dir == Path::new(".");
    let output_dir = if output_dir_was_dot {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        output_dir
    };

    let session_dir = match session_dir {
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
            let output_dir_for_plan = if output_dir_was_dot {
                Path::new(".")
            } else {
                output_dir.as_path()
            };
            match run_start_goal_without_output_dir(
                recipe.clone(),
                &backend,
                &event_tx,
                &answer_rx,
                output_dir_for_plan,
                &input,
                session_id.as_deref(),
                &model,
                &conversation_output_path,
                debug_output_path.as_deref(),
                debug,
                socket_path.as_ref(),
            ) {
                Some(p) => p,
                None => return,
            }
        }
    };

    let conversation_output_path =
        crate::resolve_log_defaults(conversation_output_path, debug_output_path, &session_dir);

    let cs_pre = read_changeset(&session_dir).ok();
    // Use repo_path from changeset for resume from any directory; fall back to resolved output_dir.
    let effective_output_dir = cs_pre
        .as_ref()
        .and_then(|c| c.repo_path.as_ref())
        .map(PathBuf::from)
        .unwrap_or_else(|| output_dir.clone());

    let start_tag = recipe.start_goal().as_str().to_string();
    let start_goal_needs_completion = cs_pre.as_ref().is_some_and(|c| {
        let st = c.state.current.as_str();
        let at_initial =
            st == recipe.initial_state().as_str() || (recipe.name() == "tdd" && st == "Init");
        recipe.uses_primary_session_document()
            && at_initial
            && (recipe
                .read_primary_session_document_utf8(&session_dir)
                .is_none()
                || get_session_for_tag(c, start_tag.as_str()).is_none())
    });
    log::debug!(
        "[workflow_runner] start_goal_needs_completion={}",
        start_goal_needs_completion
    );
    if start_goal_needs_completion {
        let input = cs_pre
            .as_ref()
            .and_then(|c| c.initial_prompt.as_deref())
            .unwrap_or("feature")
            .trim()
            .to_string();
        if !input.is_empty() {
            let storage_dir = crate::workflow::session::workflow_engine_storage_dir(&session_dir);
            std::fs::create_dir_all(&storage_dir).ok();
            let hooks = recipe.create_hooks(Some(event_tx.clone()));
            let engine =
                WorkflowEngine::new(recipe.clone(), backend.clone(), storage_dir, Some(hooks));
            let mut ctx = std::collections::HashMap::new();
            ctx.insert("feature_input".to_string(), serde_json::json!(input));
            ctx.insert(
                "output_dir".to_string(),
                serde_json::to_value(effective_output_dir.clone()).unwrap(),
            );
            ctx.insert(
                "session_dir".to_string(),
                serde_json::to_value(session_dir.clone()).unwrap(),
            );
            insert_engine_io_and_demo_flags(
                &mut ctx,
                &model,
                inherit_stdin,
                &conversation_output_path,
                debug,
                None,
                socket_path.as_ref(),
            );
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let sg = recipe.start_goal().clone();
            let result = match rt.block_on(engine.run_goal(&sg, ctx)) {
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
                    recipe: &recipe,
                    event_tx: &event_tx,
                    answer_rx: &answer_rx,
                    rt: &rt,
                    backend: &backend,
                    model: &model,
                    inherit_stdin,
                    conversation_output_path: &conversation_output_path,
                    debug,
                    socket_path: socket_path.as_ref(),
                };
                if !handle_elicitation(event, &session_dir, &elicitation_ctx) {
                    return;
                }
            }
        }
    }

    let cs = read_changeset(&session_dir).ok();
    // Feature text for PlanTask: UI prompt wins; otherwise resume from changeset when session_dir was set without initial_prompt.
    let mut feature_input_for_workflow = initial_prompt_for_ctx.clone().or_else(|| {
        cs.as_ref()
            .and_then(|c| c.initial_prompt.clone())
            .filter(|s| !s.trim().is_empty())
    });
    let start_goal = match cs.as_ref() {
        Some(c) => crate::changeset::start_goal_for_session_continue(recipe.as_ref(), c),
        None => recipe.start_goal(),
    };
    let start_is_full = start_goal == recipe.start_goal();

    // Same as the no-session_dir path: allow TUI FeatureInput when plan requires a description.
    if start_goal == recipe.start_goal() && feature_input_for_workflow.is_none() {
        let _ = event_tx.send(WorkflowEvent::AwaitingFeatureInput);
        match answer_rx.recv() {
            Ok(s) => {
                let t = s.trim().to_string();
                if t.is_empty() {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(
                        "empty feature description".into(),
                    )));
                    return;
                }
                feature_input_for_workflow = Some(t);
            }
            Err(_) => return,
        }
    }

    // Pre-set worktree_dir (from Presenter) takes priority; otherwise use changeset value (resume).
    let worktree_dir = worktree_dir.or_else(|| {
        cs.as_ref()
            .and_then(|c| c.worktree.as_ref())
            .map(PathBuf::from)
    });

    let storage_dir = crate::workflow::session::workflow_engine_storage_dir(&session_dir);
    std::fs::create_dir_all(&storage_dir).ok();
    let hooks = recipe.create_hooks(Some(event_tx.clone()));
    let backend_for_refine = backend.clone();
    let engine = WorkflowEngine::new(recipe.clone(), backend, storage_dir, Some(hooks));

    let mut context_values = std::collections::HashMap::new();
    context_values.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    context_values.insert(
        "output_dir".to_string(),
        serde_json::to_value(worktree_dir.as_ref().unwrap_or(&effective_output_dir)).unwrap(),
    );
    if let Some(ref wt) = worktree_dir {
        context_values.insert(
            "worktree_dir".to_string(),
            serde_json::to_value(wt).unwrap(),
        );
    }
    if let Some(ref fi) = feature_input_for_workflow {
        context_values.insert("feature_input".to_string(), serde_json::json!(fi));
    }
    insert_engine_io_and_demo_flags(
        &mut context_values,
        &model,
        inherit_stdin,
        &conversation_output_path,
        debug,
        Some(false),
        socket_path.as_ref(),
    );
    context_values.insert("run_optional_step_x".to_string(), serde_json::json!(false));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let result = if start_is_full {
        rt.block_on(engine.run_full_workflow(context_values))
    } else {
        rt.block_on(engine.run_workflow_from(&start_goal, context_values))
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
                    .and_then(|o| recipe.summarize_last_goal_output(o))
                    .map(|s| format!("Session dir: {}\n{}", session_dir.display(), s))
                    .unwrap_or_else(|| format!("Session dir: {}", session_dir.display()));
                let payload = WorkflowCompletePayload {
                    summary,
                    session_dir: Some(session_dir.clone()),
                };
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Ok(payload)));
                return;
            }
            ExecutionStatus::ElicitationNeeded { ref event } => {
                let elicitation_ctx = ElicitationContext {
                    recipe: &recipe,
                    event_tx: &event_tx,
                    answer_rx: &answer_rx,
                    rt: &rt,
                    backend: &backend_for_refine,
                    model: &model,
                    inherit_stdin,
                    conversation_output_path: &conversation_output_path,
                    debug,
                    socket_path: socket_path.as_ref(),
                };
                if !handle_elicitation(event, &session_dir, &elicitation_ctx) {
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
                log::debug!(
                    target: "tddy_core::presenter::workflow_runner",
                    "workflow paused; optional branch flags come from recipe + tddy-tools set-session-context (no presenter demo-plan special case)"
                );
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
