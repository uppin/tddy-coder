//! tddy-coder CLI binary.

mod plain;
mod tui;

use anyhow::Context;
use clap::Parser;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use tddy_core::{
    next_goal_for_state, read_changeset, AcceptanceTestsOptions, AnyBackend, ClaudeCodeBackend,
    CodingBackend, CursorBackend, GreenOptions, PlanOptions, ProgressEvent, RedOptions,
    ValidateOptions, Workflow, WorkflowError, WorkflowState,
};

use crate::tui::event::TuiEvent;
use crate::tui::state::should_run_tui;

#[derive(Parser, Debug)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
struct Args {
    /// Goal to execute: plan, acceptance-tests, red, green, validate-changes. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green", "validate-changes"])]
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

    /// Print Claude CLI command and cwd before running (for debugging empty output)
    #[arg(long)]
    debug: bool,

    /// Agent backend: claude or cursor (default: claude)
    #[arg(long, default_value = "claude", value_parser = ["claude", "cursor"])]
    agent: String,

    /// Feature description (alternative to stdin). When set, skips interactive/piped input.
    #[arg(long)]
    prompt: Option<String>,
}

fn main() -> anyhow::Result<()> {
    ctrlc::set_handler(|| {
        tddy_core::kill_child_process();
        std::process::exit(130);
    })
    .expect("failed to set Ctrl+C handler");

    let args = Args::parse();
    let shutdown = AtomicBool::new(false);

    if args.goal.is_none() {
        let use_tui = should_run_tui(io::stdin().is_terminal(), io::stderr().is_terminal());
        if use_tui {
            return run_full_workflow_tui(&args, &shutdown);
        }
        return run_full_workflow_plain(&args);
    }

    if args.goal.as_deref() == Some("acceptance-tests") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;

        let mut workflow = create_workflow(&args.agent);
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", args.model.as_deref().unwrap_or("sonnet"));
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = AcceptanceTestsOptions {
                model: args.model.clone(),
                agent_output: true,
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
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", args.model.as_deref().unwrap_or("sonnet"));
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = GreenOptions {
                model: args.model.clone(),
                agent_output: true,
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

    if args.goal.as_deref() == Some("validate-changes") {
        let working_dir = &args.output_dir;
        eprintln!(
            "[tddy-coder] --agent={} (from CLI, default: claude)",
            args.agent
        );
        let mut workflow = create_workflow(&args.agent);
        eprintln!("[tddy-coder] backend: {}", workflow.backend().name());
        let options = ValidateOptions {
            model: args.model.clone(),
            agent_output: true,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin: io::stdin().is_terminal(),
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.validate(working_dir, args.plan_dir.as_deref(), None, &options);
        match result {
            Ok(output) => {
                println!("{}", output.summary);
                println!("Risk level: {}", output.risk_level);
                let report_path = working_dir.join("validation-report.md");
                println!("Report: {}", report_path.display());
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
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", args.model.as_deref().unwrap_or("sonnet"));
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = RedOptions {
                model: args.model.clone(),
                agent_output: true,
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

    if args.goal.as_deref() != Some("plan") {
        anyhow::bail!(
            "unsupported goal: {}",
            args.goal.as_deref().unwrap_or("(none)")
        );
    }

    let mut input = read_feature_input(&args).context("read feature description")?;
    input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description");
    }

    let mut workflow = create_workflow(&args.agent);
    eprintln!("agent: {}", workflow.backend().name());
    eprintln!("model: {}", args.model.as_deref().unwrap_or("opus"));

    let inherit_stdin = io::stdin().is_terminal();
    let mut answers: Option<String> = None;
    loop {
        let options = PlanOptions {
            model: args.model.clone(),
            agent_output: true,
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

fn on_progress(event: &ProgressEvent) {
    let dim = "\x1b[2m";
    let reset = "\x1b[0m";
    let prefix = "\n";
    match event {
        ProgressEvent::ToolUse {
            name,
            detail: Some(d),
        } => eprintln!("{}  {}📎 {} {}...{}", prefix, dim, name, d, reset),
        ProgressEvent::ToolUse { name, detail: None } => {
            eprintln!("{}  {}📎 {}...{}", prefix, dim, name, reset)
        }
        ProgressEvent::TaskStarted { description } => {
            eprintln!("{}  {}▶ {}...{}", prefix, dim, description, reset)
        }
        ProgressEvent::TaskProgress {
            description,
            last_tool: Some(tool),
        } => eprintln!("{}  {}⏳ {} ({}){}", prefix, dim, description, tool, reset),
        ProgressEvent::TaskProgress {
            description,
            last_tool: None,
        } => eprintln!("{}  {}⏳ {}...{}", prefix, dim, description, reset),
    }
}

fn create_workflow(agent: &str) -> Workflow<AnyBackend> {
    let backend: AnyBackend = match agent {
        "cursor" => AnyBackend::Cursor(CursorBackend::new().with_progress(on_progress)),
        _ => AnyBackend::Claude(ClaudeCodeBackend::new().with_progress(on_progress)),
    };
    Workflow::new(backend).with_on_state_change(|from, to| eprintln!("State: {} → {}", from, to))
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

fn run_full_workflow_tui(args: &Args, shutdown: &AtomicBool) -> anyhow::Result<()> {
    std::env::set_var("TDDY_QUIET", "1");
    let (event_tx, event_rx) = mpsc::channel();
    let (answer_tx, answer_rx) = mpsc::sync_channel(0);

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

    let event_tx_clone = event_tx.clone();
    let workflow_handle = thread::spawn(move || {
        run_workflow_thread(thread_args, event_tx_clone, answer_rx)
    });

    let result = tui::run::run_tui_event_loop(event_rx, answer_tx, shutdown);

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
    let inherit_stdin = true;

    let mut workflow = create_workflow_for_tui(&agent, event_tx.clone());
    event_tx.send(TuiEvent::GoalStarted("plan".to_string())).ok();

    let plan_dir = if let Some(ref plan_dir) = plan_dir_arg {
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
            let _ = event_tx.send(TuiEvent::WorkflowComplete(Err("empty feature description".into())));
            return;
        }
        let plan_options = PlanOptions {
            model: model.clone(),
            agent_output: true,
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
        event_tx.send(TuiEvent::GoalStarted("acceptance-tests".to_string())).ok();
        let at_options = AcceptanceTestsOptions {
            model: model.clone(),
            agent_output: true,
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

    event_tx.send(TuiEvent::GoalStarted("green".to_string())).ok();
    let green_options = GreenOptions {
        model: model.clone(),
        agent_output: true,
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
                let summary = format!(
                    "{}\nPlan dir: {}",
                    output.summary,
                    plan_dir.display()
                );
                let _ = event_tx.send(TuiEvent::WorkflowComplete(Ok(summary)));
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
    let model = args.model.as_deref().unwrap_or("opus");
    let workflow = create_workflow(&args.agent);
    eprintln!("agent: {}", workflow.backend().name());
    eprintln!("model: {}", model);

    let inherit_stdin = io::stdin().is_terminal();

    let plan_dir = if let Some(ref plan_dir) = args.plan_dir {
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

/// Read feature description. Uses --prompt if set; otherwise stdin.
fn read_feature_input(args: &Args) -> anyhow::Result<String> {
    if let Some(ref p) = args.prompt {
        return Ok(p.clone());
    }
    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf)?;
    Ok(buf)
}
