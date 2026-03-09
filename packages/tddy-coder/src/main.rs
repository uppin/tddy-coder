//! tddy-coder CLI binary.

mod plain;
mod tui;

use anyhow::Context;
use clap::Parser;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tddy_core::{
    get_session_for_tag, init_tddy_logger, next_goal_for_state, read_changeset,
    AcceptanceTestsOptions, AgentOutputSink, AnyBackend, ClaudeCodeBackend, CursorBackend,
    DemoOptions, EvaluateOptions, GreenOptions, PlanOptions, ProgressEvent, RedOptions,
    RefactorOptions, ValidateOptions, Workflow, WorkflowError, WorkflowState,
};

use crate::tui::event::TuiEvent;
use crate::tui::state::should_run_tui;

#[derive(Parser, Debug)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
struct Args {
    /// Goal to execute: plan, acceptance-tests, red, green, demo, evaluate, validate, refactor. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "demo", "evaluate", "validate", "refactor"])]
    goal: Option<String>,

    /// Output directory for planning artifacts (default: current directory)
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    /// Plan directory (required when goal is acceptance-tests, red, or green)
    #[arg(long)]
    plan_dir: Option<PathBuf>,

    /// Write entire agent conversation (raw bytes) to file
    #[arg(long)]
    conversation_output: Option<PathBuf>,

    /// Model name for Claude Code CLI (e.g. sonnet)
    #[arg(short, long)]
    model: Option<String>,

    /// Extra tools to add to the goal's allowlist (comma-separated, e.g. "Bash(npm install)")
    #[arg(long, value_delimiter = ',')]
    allowed_tools: Option<Vec<String>>,

    /// Enable debug logging (stderr in plain mode, TUI debug area in TUI mode)
    #[arg(long)]
    debug: bool,

    /// Enable debug logging and redirect to file (avoids stderr/TUI corruption)
    #[arg(long)]
    debug_output: Option<PathBuf>,

    /// Agent backend: claude or cursor (default: claude)
    #[arg(long, default_value = "claude", value_parser = ["claude", "cursor"])]
    agent: String,

    /// Feature description (alternative to stdin). When set, skips interactive/piped input.
    #[arg(long)]
    prompt: Option<String>,
}

fn main() {
    let args = Args::parse();
    init_tddy_logger(args.debug, args.debug_output.as_deref());

    ctrlc::set_handler(|| {
        tddy_core::kill_child_process();
        // Restore terminal before exit; otherwise raw mode leaves it broken.
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = crate::tui::raw::disable_raw_mode();
        std::process::exit(130);
    })
    .expect("failed to set Ctrl+C handler");
    let shutdown = Arc::new(AtomicBool::new(false));

    let result = run(&args, shutdown);
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    if args.goal.is_none() {
        let use_tui = should_run_tui(io::stdin().is_terminal(), io::stderr().is_terminal());
        if use_tui {
            return run_full_workflow_tui(args, shutdown);
        }
        return run_full_workflow_plain(args);
    }

    log::debug!(
        "[tddy-coder] goal: {}, agent: {}, model: {}",
        args.goal.as_deref().unwrap_or("(none)"),
        args.agent,
        args.model.as_deref().unwrap_or("(default)")
    );

    if args.goal.as_deref() == Some("acceptance-tests") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;

        let mut workflow = create_workflow(&args.agent);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = AcceptanceTestsOptions {
                model: args.model.clone(),
                agent_output: true,
                agent_output_sink: None,
                conversation_output_path: args.conversation_output.clone(),
                inherit_stdin,
                allowed_tools_extras: args.allowed_tools.clone(),
                debug: args.debug,
            };
            let result = workflow.acceptance_tests(plan_dir, answers.as_deref(), &options);

            match result {
                Ok(output) => {
                    println!("{}", output.summary);
                    for t in &output.tests {
                        println!(
                            "  - {} ({}:{}): {}",
                            t.name,
                            t.file,
                            t.line.unwrap_or(0),
                            t.status
                        );
                    }
                    if let Some(cmd) = &output.test_command {
                        println!("\nHow to run tests: {}", cmd);
                    }
                    if let Some(prereq) = &output.prerequisite_actions {
                        println!("Prerequisite actions: {}", prereq);
                    }
                    if let Some(single) = &output.run_single_or_selected_tests {
                        println!("How to run a single or selected tests: {}", single);
                    }
                    println!("\nPlan dir: {}", plan_dir.display());
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if args.goal.as_deref() == Some("green") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for green goal"))?;

        let mut workflow = create_workflow(&args.agent);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = GreenOptions {
                model: args.model.clone(),
                agent_output: true,
                agent_output_sink: None,
                conversation_output_path: args.conversation_output.clone(),
                inherit_stdin,
                allowed_tools_extras: args.allowed_tools.clone(),
                debug: args.debug,
            };
            let result = workflow.green(plan_dir, answers.as_deref(), &options);

            match result {
                Ok(output) => {
                    println!("{}", output.summary);
                    for t in &output.tests {
                        println!(
                            "  - {} ({}:{}): {}",
                            t.name,
                            t.file,
                            t.line.unwrap_or(0),
                            t.status
                        );
                    }
                    for i in &output.implementations {
                        println!(
                            "  [impl] {} ({}:{}): {}",
                            i.name,
                            i.file,
                            i.line.unwrap_or(0),
                            i.kind
                        );
                    }
                    if let Some(cmd) = &output.test_command {
                        println!("\nHow to run tests: {}", cmd);
                    }
                    if let Some(prereq) = &output.prerequisite_actions {
                        println!("Prerequisite actions: {}", prereq);
                    }
                    if let Some(single) = &output.run_single_or_selected_tests {
                        println!("How to run a single or selected tests: {}", single);
                    }
                    println!("\nPlan dir: {}", plan_dir.display());
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if args.goal.as_deref() == Some("evaluate") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for evaluate"))?;
        let mut workflow = create_workflow(&args.agent);
        let options = tddy_core::EvaluateOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin: io::stdin().is_terminal(),
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.evaluate(&args.output_dir, Some(plan_dir), None, &options);
        match result {
            Ok(output) => {
                println!("{}", output.summary);
                println!("Risk level: {}", output.risk_level);
                let report_path = plan_dir.join("evaluation-report.md");
                println!("Report: {}", report_path.display());
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }

    if args.goal.as_deref() == Some("demo") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for demo goal"))?;
        let mut workflow = create_workflow(&args.agent);
        let options = tddy_core::DemoOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin: io::stdin().is_terminal(),
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.demo(plan_dir, None, &options);
        match result {
            Ok(output) => {
                println!("{}", output.summary);
                println!("Steps completed: {}", output.steps_completed);
                println!("Plan dir: {}", plan_dir.display());
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }

    if args.goal.as_deref() == Some("red") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for red goal"))?;

        let mut workflow = create_workflow(&args.agent);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = RedOptions {
                model: args.model.clone(),
                agent_output: true,
                agent_output_sink: None,
                conversation_output_path: args.conversation_output.clone(),
                inherit_stdin,
                allowed_tools_extras: args.allowed_tools.clone(),
                debug: args.debug,
            };
            let result = workflow.red(plan_dir, answers.as_deref(), &options);

            match result {
                Ok(output) => {
                    println!("{}", output.summary);
                    for t in &output.tests {
                        println!(
                            "  - {} ({}:{}): {}",
                            t.name,
                            t.file,
                            t.line.unwrap_or(0),
                            t.status
                        );
                    }
                    for s in &output.skeletons {
                        println!(
                            "  [skeleton] {} ({}:{}): {}",
                            s.name,
                            s.file,
                            s.line.unwrap_or(0),
                            s.kind
                        );
                    }
                    if let Some(cmd) = &output.test_command {
                        println!("\nHow to run tests: {}", cmd);
                    }
                    if let Some(prereq) = &output.prerequisite_actions {
                        println!("Prerequisite actions: {}", prereq);
                    }
                    if let Some(single) = &output.run_single_or_selected_tests {
                        println!("How to run a single or selected tests: {}", single);
                    }
                    println!("\nPlan dir: {}", plan_dir.display());
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if args.goal.as_deref() == Some("validate") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for validate goal"))?;
        let mut workflow = create_workflow(&args.agent);
        let options = tddy_core::ValidateOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin: io::stdin().is_terminal(),
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.validate(plan_dir, None, &options);
        match result {
            Ok(output) => {
                println!("{}", output.summary);
                println!("Plan dir: {}", plan_dir.display());
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }

    if args.goal.as_deref() == Some("refactor") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for refactor goal"))?;
        let mut workflow = create_workflow(&args.agent);
        let options = tddy_core::RefactorOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin: io::stdin().is_terminal(),
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.refactor(plan_dir, None, &options);
        match result {
            Ok(output) => {
                println!("{}", output.summary);
                println!("Tasks completed: {}", output.tasks_completed);
                println!("Tests passing: {}", output.tests_passing);
                println!("Plan dir: {}", plan_dir.display());
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }

    if args.goal.as_deref() != Some("plan") {
        anyhow::bail!(
            "unsupported goal: {}",
            args.goal.as_deref().unwrap_or("(none)")
        );
    }

    let mut input = read_feature_input(args).context("read feature description")?;
    input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }

    let mut workflow = create_workflow(&args.agent);

    let inherit_stdin = io::stdin().is_terminal();
    let mut answers: Option<String> = None;
    loop {
        let options = PlanOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.plan(&input, &args.output_dir, answers.as_deref(), &options);

        match result {
            Ok(output_path) => {
                println!("{}", output_path.display());
                return Ok(());
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn on_progress(_event: &ProgressEvent) {
    // Plain mode: progress is not displayed (no stdout/stderr per AGENTS.md)
}

fn create_workflow(agent: &str) -> Workflow<AnyBackend> {
    log::debug!("[tddy-coder] using agent: {}", agent);
    let backend: AnyBackend = match agent {
        "cursor" => AnyBackend::Cursor(CursorBackend::new().with_progress(on_progress)),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(on_progress)),
    };
    Workflow::new(backend).with_on_state_change(|from, to| {
        log::debug!("[tddy-coder] state: {} → {}", from, to);
    })
}

fn create_workflow_for_tui(agent: &str, event_tx: mpsc::Sender<TuiEvent>) -> Workflow<AnyBackend> {
    let tx = event_tx.clone();
    let progress = move |ev: &ProgressEvent| {
        let _ = tx.send(TuiEvent::Progress(ev.clone()));
    };
    let tx2 = event_tx.clone();
    let state_change = move |from: &str, to: &str| {
        let _ = tx2.send(TuiEvent::StateChange {
            from: from.to_string(),
            to: to.to_string(),
        });
    };
    let backend: AnyBackend = match agent {
        "cursor" => AnyBackend::Cursor(CursorBackend::new().with_progress(progress)),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(progress)),
    };
    Workflow::new(backend).with_on_state_change(state_change)
}

struct WorkflowThreadArgs {
    output_dir: PathBuf,
    plan_dir: Option<PathBuf>,
    conversation_output: Option<PathBuf>,
    model: Option<String>,
    allowed_tools: Option<Vec<String>>,
    debug: bool,
    agent: String,
    prompt: Option<String>,
}

fn run_full_workflow_tui(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    std::env::set_var("TDDY_QUIET", "1");
    let (event_tx, event_rx) = mpsc::channel();
    let (answer_tx, answer_rx) = mpsc::channel();

    let thread_args = WorkflowThreadArgs {
        output_dir: args.output_dir.clone(),
        plan_dir: args.plan_dir.clone(),
        conversation_output: args.conversation_output.clone(),
        model: args.model.clone(),
        allowed_tools: args.allowed_tools.clone(),
        debug: args.debug,
        agent: args.agent.clone(),
        prompt: args.prompt.clone(),
    };

    let event_tx_workflow = event_tx.clone();
    let event_tx_crossterm = event_tx.clone();
    let workflow_handle =
        thread::spawn(move || run_workflow_thread(thread_args, event_tx_workflow, answer_rx));

    let agent = args.agent.as_str();
    let model = args.model.as_deref().unwrap_or("opus");
    let result = tui::run::run_tui_event_loop(
        event_rx,
        answer_tx,
        event_tx_crossterm,
        shutdown,
        agent,
        model,
    );

    let _ = workflow_handle.join();
    result
}

fn run_workflow_thread(
    args: WorkflowThreadArgs,
    event_tx: mpsc::Sender<TuiEvent>,
    answer_rx: mpsc::Receiver<String>,
) {
    let WorkflowThreadArgs {
        output_dir,
        plan_dir: plan_dir_arg,
        conversation_output,
        model,
        allowed_tools,
        debug,
        agent,
        prompt,
    } = args;
    // Raw mode keeps ISIG so Ctrl+C generates SIGINT; agent can inherit stdin for prompts.
    let inherit_stdin = true;

    let agent_output_sink = AgentOutputSink::new({
        let tx = event_tx.clone();
        move |s: &str| {
            let _ = tx.send(TuiEvent::AgentOutput(s.to_string()));
        }
    });

    let mut workflow = create_workflow_for_tui(&agent, event_tx.clone());
    event_tx
        .send(TuiEvent::GoalStarted("plan".to_string()))
        .ok();

    let mut plan_dir = if let Some(ref plan_dir) = plan_dir_arg {
        plan_dir.clone()
    } else {
        let input = if let Some(p) = prompt {
            p
        } else {
            match answer_rx.recv() {
                Ok(s) => s,
                Err(_) => return,
            }
        };
        let input = input.trim().to_string();
        if input.is_empty() {
            let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(
                "empty feature description".into()
            )));
            return;
        }
        let plan_options = PlanOptions {
            model: model.clone(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink.clone()),
            conversation_output_path: conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: allowed_tools.clone(),
            debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.plan(&input, &output_dir, answers.as_deref(), &plan_options);
            match result {
                Ok(output_path) => break output_path,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    let _ = event_tx.send(TuiEvent::ClarificationNeeded { questions });
                    match answer_rx.recv() {
                        Ok(a) => answers = Some(a),
                        Err(_) => return,
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            }
        }
    };

    // When resuming with --plan-dir: if state is Init and plan is incomplete (no PRD or no plan
    // session), run plan to complete it using initial_prompt from changeset.
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
                conversation_output_path: conversation_output.clone(),
                inherit_stdin,
                allowed_tools_extras: allowed_tools.clone(),
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
                        let _ = event_tx.send(TuiEvent::ClarificationNeeded { questions });
                        match answer_rx.recv() {
                            Ok(a) => answers = Some(a),
                            Err(_) => return,
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
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
            .send(TuiEvent::GoalStarted("acceptance-tests".to_string()))
            .ok();
        let at_options = AcceptanceTestsOptions {
            model: model.clone(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink.clone()),
            conversation_output_path: conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: allowed_tools.clone(),
            debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.acceptance_tests(&plan_dir, answers.as_deref(), &at_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    let _ = event_tx.send(TuiEvent::ClarificationNeeded { questions });
                    match answer_rx.recv() {
                        Ok(a) => answers = Some(a),
                        Err(_) => return,
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            }
        }
    }

    if run_red {
        event_tx.send(TuiEvent::GoalStarted("red".to_string())).ok();
        let red_options = RedOptions {
            model: model.clone(),
            agent_output: true,
            agent_output_sink: Some(agent_output_sink.clone()),
            conversation_output_path: conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: allowed_tools.clone(),
            debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.red(&plan_dir, answers.as_deref(), &red_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    let _ = event_tx.send(TuiEvent::ClarificationNeeded { questions });
                    match answer_rx.recv() {
                        Ok(a) => answers = Some(a),
                        Err(_) => return,
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                    return;
                }
            }
        }
    }

    event_tx
        .send(TuiEvent::GoalStarted("green".to_string()))
        .ok();
    let green_options = GreenOptions {
        model: model.clone(),
        agent_output: true,
        agent_output_sink: Some(agent_output_sink.clone()),
        conversation_output_path: conversation_output.clone(),
        inherit_stdin,
        allowed_tools_extras: allowed_tools.clone(),
        debug,
    };
    let mut answers: Option<String> = None;
    loop {
        let result = workflow.green(&plan_dir, answers.as_deref(), &green_options);
        match result {
            Ok(output) => {
                // After green: demo (if demo-plan.md exists) then evaluate
                let run_demo = if plan_dir.join("demo-plan.md").exists() {
                    event_tx.send(TuiEvent::DemoPrompt).ok();
                    match answer_rx.recv() {
                        Ok(choice) => choice.eq_ignore_ascii_case("run"),
                        Err(_) => return,
                    }
                } else {
                    false
                };
                if run_demo {
                    event_tx
                        .send(TuiEvent::GoalStarted("demo".to_string()))
                        .ok();
                    match workflow.demo(&plan_dir, None, &DemoOptions::default()) {
                        Ok(_) => {}
                        Err(e) => {
                            let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                            return;
                        }
                    }
                }
                event_tx
                    .send(TuiEvent::GoalStarted("evaluate".to_string()))
                    .ok();
                let eval_options = EvaluateOptions {
                    model: model.clone(),
                    agent_output: true,
                    agent_output_sink: Some(agent_output_sink.clone()),
                    conversation_output_path: conversation_output.clone(),
                    inherit_stdin,
                    allowed_tools_extras: allowed_tools.clone(),
                    debug,
                };
                match workflow.evaluate(&output_dir, Some(&plan_dir), None, &eval_options) {
                    Ok(eval_out) => {
                        event_tx
                            .send(TuiEvent::GoalStarted("validate".to_string()))
                            .ok();
                        let validate_options = ValidateOptions {
                            model: model.clone(),
                            agent_output: true,
                            agent_output_sink: Some(agent_output_sink.clone()),
                            conversation_output_path: conversation_output.clone(),
                            inherit_stdin,
                            allowed_tools_extras: allowed_tools.clone(),
                            debug,
                        };
                        match workflow.validate(&plan_dir, None, &validate_options) {
                            Ok(validate_out) => {
                                event_tx
                                    .send(TuiEvent::GoalStarted("refactor".to_string()))
                                    .ok();
                                let refactor_options = RefactorOptions {
                                    model: model.clone(),
                                    agent_output: true,
                                    agent_output_sink: Some(agent_output_sink.clone()),
                                    conversation_output_path: conversation_output.clone(),
                                    inherit_stdin,
                                    allowed_tools_extras: allowed_tools.clone(),
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
                                        let _ =
                                            event_tx.send(TuiEvent::WorkflowComplete(Ok(summary)));
                                    }
                                    Err(e) => {
                                        let _ = event_tx
                                            .send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ =
                                    event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                    }
                }
                return;
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                let _ = event_tx.send(TuiEvent::ClarificationNeeded { questions });
                match answer_rx.recv() {
                    Ok(a) => answers = Some(a),
                    Err(_) => return,
                }
            }
            Err(e) => {
                let _ = event_tx.send(TuiEvent::WorkflowComplete(Err(e.to_string())));
                return;
            }
        }
    }
}

fn run_full_workflow_plain(args: &Args) -> anyhow::Result<()> {
    let inherit_stdin = io::stdin().is_terminal();

    let mut plan_dir = if let Some(ref plan_dir) = args.plan_dir {
        plan_dir.clone()
    } else {
        let mut input = read_feature_input(args).context("read feature description")?;
        input = input.trim().to_string();
        if input.is_empty() {
            anyhow::bail!("empty feature description");
        }
        let mut workflow = create_workflow(&args.agent);
        let plan_options = PlanOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.plan(&input, &args.output_dir, answers.as_deref(), &plan_options);
            match result {
                Ok(output_path) => break output_path,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    };

    // When resuming with --plan-dir: if state is Init and plan is incomplete, run plan to complete it.
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
                .unwrap_or_else(|| args.output_dir.clone());
            let mut workflow = create_workflow(&args.agent);
            let plan_options = PlanOptions {
                model: args.model.clone(),
                agent_output: true,
                agent_output_sink: None,
                conversation_output_path: args.conversation_output.clone(),
                inherit_stdin,
                allowed_tools_extras: args.allowed_tools.clone(),
                debug: args.debug,
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
                        answers =
                            Some(plain::read_answers_plain(&questions).context("read answers")?);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    let mut workflow = create_workflow(&args.agent);
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
        let at_options = AcceptanceTestsOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.acceptance_tests(&plan_dir, answers.as_deref(), &at_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if run_red {
        let red_options = RedOptions {
            model: args.model.clone(),
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let mut answers: Option<String> = None;
        loop {
            let result = workflow.red(&plan_dir, answers.as_deref(), &red_options);
            match result {
                Ok(_) => break,
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    let green_options = GreenOptions {
        model: args.model.clone(),
        agent_output: true,
        agent_output_sink: None,
        conversation_output_path: args.conversation_output.clone(),
        inherit_stdin,
        allowed_tools_extras: args.allowed_tools.clone(),
        debug: args.debug,
    };
    let mut answers: Option<String> = None;
    loop {
        let result = workflow.green(&plan_dir, answers.as_deref(), &green_options);
        match result {
            Ok(output) => {
                // After green: demo (if demo-plan.md exists) then evaluate
                let run_demo = plan_dir.join("demo-plan.md").exists()
                    && plain::read_demo_choice_plain().context("read demo choice")?;
                if run_demo {
                    workflow.demo(&plan_dir, None, &DemoOptions::default())?;
                }
                let eval_options = EvaluateOptions {
                    model: args.model.clone(),
                    agent_output: true,
                    agent_output_sink: None,
                    conversation_output_path: args.conversation_output.clone(),
                    inherit_stdin,
                    allowed_tools_extras: args.allowed_tools.clone(),
                    debug: args.debug,
                };
                let eval_out =
                    workflow.evaluate(&args.output_dir, Some(&plan_dir), None, &eval_options)?;

                let validate_options = ValidateOptions {
                    model: args.model.clone(),
                    agent_output: true,
                    agent_output_sink: None,
                    conversation_output_path: args.conversation_output.clone(),
                    inherit_stdin,
                    allowed_tools_extras: args.allowed_tools.clone(),
                    debug: args.debug,
                };
                let validate_out = workflow.validate(&plan_dir, None, &validate_options)?;

                let refactor_options = RefactorOptions {
                    model: args.model.clone(),
                    agent_output: true,
                    agent_output_sink: None,
                    conversation_output_path: args.conversation_output.clone(),
                    inherit_stdin,
                    allowed_tools_extras: args.allowed_tools.clone(),
                    debug: args.debug,
                };
                let refactor_out = workflow.refactor(&plan_dir, None, &refactor_options)?;

                println!("{}", output.summary);
                for t in &output.tests {
                    println!(
                        "  - {} ({}:{}): {}",
                        t.name,
                        t.file,
                        t.line.unwrap_or(0),
                        t.status
                    );
                }
                for i in &output.implementations {
                    println!(
                        "  [impl] {} ({}:{}): {}",
                        i.name,
                        i.file,
                        i.line.unwrap_or(0),
                        i.kind
                    );
                }
                if let Some(cmd) = &output.test_command {
                    println!("\nHow to run tests: {}", cmd);
                }
                if let Some(prereq) = &output.prerequisite_actions {
                    println!("Prerequisite actions: {}", prereq);
                }
                if let Some(single) = &output.run_single_or_selected_tests {
                    println!("How to run a single or selected tests: {}", single);
                }
                println!("\nEvaluation: {}", eval_out.summary);
                println!("{}", validate_out.summary);
                println!("{}", refactor_out.summary);
                println!("Tasks completed: {}", refactor_out.tasks_completed);
                println!("Tests passing: {}", refactor_out.tests_passing);
                println!("\nPlan dir: {}", plan_dir.display());
                return Ok(());
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                answers = Some(plain::read_answers_plain(&questions).context("read answers")?);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Read feature description. Uses --prompt if set; otherwise stdin.
fn read_feature_input(args: &Args) -> anyhow::Result<String> {
    if let Some(ref p) = args.prompt {
        return Ok(p.clone());
    }
    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf)?;
    Ok(buf)
}
