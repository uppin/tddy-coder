//! Workflow thread runner — runs the full TDD workflow and sends events.

use std::path::PathBuf;
use std::sync::mpsc;

use crate::{
    get_session_for_tag, next_goal_for_state, read_changeset, AcceptanceTestsOptions,
    AgentOutputSink, ClarificationQuestion, EvaluateOptions, GreenOptions, PlanOptions,
    QuestionOption, RedOptions, RefactorOptions, SharedBackend, ValidateOptions, Workflow,
    WorkflowError, WorkflowState,
};

use super::WorkflowEvent;

fn demo_question() -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Demo".to_string(),
        question: "Create & run a demo?".to_string(),
        options: vec![
            QuestionOption {
                label: "Create & run".to_string(),
                description: "Create and run the demo script".to_string(),
            },
            QuestionOption {
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

    let agent_output_sink = AgentOutputSink::new({
        let tx = event_tx.clone();
        move |s: &str| {
            let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
        }
    });

    let tx_sc = event_tx.clone();
    let state_change = move |from: &str, to: &str| {
        let _ = tx_sc.send(WorkflowEvent::StateChange {
            from: from.to_string(),
            to: to.to_string(),
        });
    };
    let mut workflow = Workflow::new(backend).with_on_state_change(state_change);

    let mut plan_dir = match plan_dir {
        Some(p) => p,
        None => {
            // Wait for feature input BEFORE sending GoalStarted, so the TUI stays in
            // FeatureInput mode until the user submits. Otherwise GoalStarted would
            // switch to Running, and Enter would send QueuePrompt instead of SubmitFeatureInput.
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
            event_tx
                .send(WorkflowEvent::GoalStarted("plan".to_string()))
                .ok();
            let plan_options = PlanOptions {
                model: model.clone(),
                agent_output: true,
                agent_output_sink: Some(agent_output_sink.clone()),
                conversation_output_path: None,
                inherit_stdin,
                allowed_tools_extras: None,
                debug,
            };
            let mut answers: Option<String> = None;
            loop {
                let result = workflow.plan(&input, &output_dir, answers.as_deref(), &plan_options);
                match result {
                    Ok(output_path) => break output_path,
                    Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                        let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                        match answer_rx.recv() {
                            Ok(a) => answers = Some(a),
                            Err(_) => return,
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return;
                    }
                }
            }
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
            let plan_output_dir = plan_dir
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| output_dir.clone());
            let plan_options = PlanOptions {
                model: model.clone(),
                agent_output: true,
                agent_output_sink: Some(agent_output_sink.clone()),
                conversation_output_path: None,
                inherit_stdin,
                allowed_tools_extras: None,
                debug,
            };
            let mut answers: Option<String> = None;
            loop {
                let result =
                    workflow.plan(&input, &plan_output_dir, answers.as_deref(), &plan_options);
                match result {
                    Ok(output_path) => {
                        plan_dir = output_path;
                        break;
                    }
                    Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                        let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                        match answer_rx.recv() {
                            Ok(a) => answers = Some(a),
                            Err(_) => return,
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return;
                    }
                }
            }
        }
    }

    let cs = read_changeset(&plan_dir).ok();
    let start_goal = cs
        .as_ref()
        .and_then(|c| next_goal_for_state(&c.state.current))
        .unwrap_or("plan");

    let run_acceptance_tests = matches!(start_goal, "plan" | "acceptance-tests");
    let run_red = matches!(start_goal, "plan" | "acceptance-tests" | "red");

    if run_acceptance_tests {
        if cs.as_ref().map(|c| c.state.current.as_str()) == Some("Planned") {
            workflow.restore_state(WorkflowState::Planned {
                output_dir: plan_dir.to_path_buf(),
            });
        }
        event_tx
            .send(WorkflowEvent::GoalStarted("acceptance-tests".to_string()))
            .ok();
        let at_options = AcceptanceTestsOptions {
            model: model.clone(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink.clone()),
            conversation_output_path: None,
            inherit_stdin,
            allowed_tools_extras: None,
            debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.acceptance_tests(&plan_dir, answers.as_deref(), &at_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                    match answer_rx.recv() {
                        Ok(a) => answers = Some(a),
                        Err(_) => return,
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            }
        }
    }

    if run_red {
        event_tx
            .send(WorkflowEvent::GoalStarted("red".to_string()))
            .ok();
        let red_options = RedOptions {
            model: model.clone(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink.clone()),
            conversation_output_path: None,
            inherit_stdin,
            allowed_tools_extras: None,
            debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.red(&plan_dir, answers.as_deref(), &red_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                    match answer_rx.recv() {
                        Ok(a) => answers = Some(a),
                        Err(_) => return,
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            }
        }
    }

    event_tx
        .send(WorkflowEvent::GoalStarted("green".to_string()))
        .ok();
    let green_options = GreenOptions {
        model: model.clone(),
        agent_output: true,
        agent_output_sink: Some(agent_output_sink.clone()),
        conversation_output_path: None,
        inherit_stdin,
        allowed_tools_extras: None,
        debug,
    };
    let mut answers: Option<String> = None;
    loop {
        let result = workflow.green(&plan_dir, answers.as_deref(), &green_options);
        match result {
            Ok(output) => {
                let run_demo = if plan_dir.join("demo-plan.md").exists() {
                    let _ = event_tx.send(WorkflowEvent::ClarificationNeeded {
                        questions: vec![demo_question()],
                    });
                    match answer_rx.recv() {
                        Ok(choice) => !choice.eq_ignore_ascii_case("skip"),
                        Err(_) => return,
                    }
                } else {
                    false
                };
                if run_demo {
                    event_tx
                        .send(WorkflowEvent::GoalStarted("demo".to_string()))
                        .ok();
                    if let Err(e) = workflow.demo(&plan_dir, None, &crate::DemoOptions::default()) {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                        return;
                    }
                }
                event_tx
                    .send(WorkflowEvent::GoalStarted("evaluate".to_string()))
                    .ok();
                let eval_options = EvaluateOptions {
                    model: model.clone(),
                    agent_output: true,
                    agent_output_sink: Some(agent_output_sink.clone()),
                    conversation_output_path: None,
                    inherit_stdin,
                    allowed_tools_extras: None,
                    debug,
                };
                match workflow.evaluate(&output_dir, Some(&plan_dir), None, &eval_options) {
                    Ok(eval_out) => {
                        event_tx
                            .send(WorkflowEvent::GoalStarted("validate".to_string()))
                            .ok();
                        let validate_options = ValidateOptions {
                            model: model.clone(),
                            agent_output: true,
                            agent_output_sink: Some(agent_output_sink.clone()),
                            conversation_output_path: None,
                            inherit_stdin,
                            allowed_tools_extras: None,
                            debug,
                        };
                        match workflow.validate(&plan_dir, None, &validate_options) {
                            Ok(validate_out) => {
                                // StubBackend doesn't write files; write refactoring-plan.md
                                // so refactor can proceed (in production validate agent writes it).
                                let refactoring_plan_path = plan_dir.join("refactoring-plan.md");
                                if !refactoring_plan_path.exists() {
                                    let _ = std::fs::write(
                                        &refactoring_plan_path,
                                        "# Refactoring Plan\n## Tasks\n1. No-op refactoring task\n",
                                    );
                                }

                                event_tx
                                    .send(WorkflowEvent::GoalStarted("refactor".to_string()))
                                    .ok();
                                let refactor_options = RefactorOptions {
                                    model: model.clone(),
                                    agent_output: true,
                                    agent_output_sink: Some(agent_output_sink.clone()),
                                    conversation_output_path: None,
                                    inherit_stdin,
                                    allowed_tools_extras: None,
                                    debug,
                                };
                                match workflow.refactor(&plan_dir, None, &refactor_options) {
                                    Ok(refactor_out) => {
                                        let summary = format!(
                                            "{}\nPlan dir: {}\nEvaluation: {}\n{}\n{}\nTasks completed: {}\nTests passing: {}",
                                            output.summary,
                                            plan_dir.display(),
                                            eval_out.summary,
                                            validate_out.summary,
                                            refactor_out.summary,
                                            refactor_out.tasks_completed,
                                            refactor_out.tests_passing
                                        );
                                        let _ = event_tx
                                            .send(WorkflowEvent::WorkflowComplete(Ok(summary)));
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(
                                            Err(e.to_string()),
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                    }
                }
                return;
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                let _ = event_tx.send(WorkflowEvent::ClarificationNeeded { questions });
                match answer_rx.recv() {
                    Ok(a) => answers = Some(a),
                    Err(_) => return,
                }
            }
            Err(e) => {
                let _ = event_tx.send(WorkflowEvent::WorkflowComplete(Err(e.to_string())));
                return;
            }
        }
    }
}
