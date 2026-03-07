//! tddy-coder CLI binary.

use anyhow::Context;
use clap::Parser;
use inquire::{MultiSelect, Select, Text};
use std::io::{self, BufRead, IsTerminal, Read};
use std::path::{Path, PathBuf};
use tddy_core::{
    next_goal_for_state, read_changeset, AcceptanceTestsOptions, AnyBackend, ClarificationQuestion,
    ClaudeCodeBackend, CodingBackend, CursorBackend, GreenOptions, PlanOptions, ProgressEvent,
    RedOptions, Workflow, WorkflowError, WorkflowState,
};

#[derive(Parser, Debug)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
struct Args {
    /// Goal to execute: plan, acceptance-tests, red, green. Omit to run full workflow.
    #[arg(long, value_parser = ["plan", "acceptance-tests", "red", "green"])]
    goal: Option<String>,

    /// Output directory for planning artifacts (default: current directory)
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    /// Plan directory (required when goal is acceptance-tests, red, or green)
    #[arg(long)]
    plan_dir: Option<PathBuf>,

    /// Print raw agent output to stderr in real-time
    #[arg(long)]
    agent_output: bool,

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
    let args = Args::parse();

    if args.goal.is_none() {
        return run_full_workflow(&args);
    }

    if args.goal.as_deref() == Some("acceptance-tests") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;

        let model = args.model.as_deref().unwrap_or("sonnet");
        let mut workflow = create_workflow(&args.agent);
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", model);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = AcceptanceTestsOptions {
                model: args.model.clone(),
                agent_output: args.agent_output,
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
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(read_answers(&questions).context("read answers")?);
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

        let model = args.model.as_deref().unwrap_or("sonnet");
        let mut workflow = create_workflow(&args.agent);
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", model);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = GreenOptions {
                model: args.model.clone(),
                agent_output: args.agent_output,
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
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(read_answers(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if args.goal.as_deref() == Some("red") {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for red goal"))?;

        let model = args.model.as_deref().unwrap_or("sonnet");
        let mut workflow = create_workflow(&args.agent);
        eprintln!("agent: {}", workflow.backend().name());
        eprintln!("model: {}", model);
        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = RedOptions {
                model: args.model.clone(),
                agent_output: args.agent_output,
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
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(read_answers(&questions).context("read answers")?);
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

    let model = args.model.as_deref().unwrap_or("opus");
    let mut workflow = create_workflow(&args.agent);
    eprintln!("agent: {}", workflow.backend().name());
    eprintln!("model: {}", model);

    let inherit_stdin = io::stdin().is_terminal();
    let mut answers: Option<String> = None;
    loop {
        let options = PlanOptions {
            model: args.model.clone(),
            agent_output: args.agent_output,
            conversation_output_path: args.conversation_output.clone(),
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
            debug: args.debug,
        };
        let result = workflow.plan(&input, &args.output_dir, answers.as_deref(), &options);

        match result {
            Ok(output_path) => {
                let prd_path = output_path.join("PRD.md");
                println!("{}", prd_path.display());
                return Ok(());
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                answers = Some(read_answers(&questions).context("read answers")?);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn on_progress(event: &ProgressEvent) {
    let dim = "\x1b[2m";
    let reset = "\x1b[0m";
    // Ensure progress lines start on a new line (agent_output may have printed text without newline).
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

/// Scan output_dir for the most recently modified subdirectory containing changeset.yaml.
fn find_resumable_plan_dir(output_dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(output_dir).ok()?;
    let mut candidates: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let changeset_path = path.join("changeset.yaml");
            if changeset_path.exists() {
                if let Ok(meta) = path.metadata() {
                    if let Ok(modified) = meta.modified() {
                        candidates.push((path, modified));
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.into_iter().next().map(|(p, _)| p)
}

fn run_full_workflow(args: &Args) -> anyhow::Result<()> {
    let model = args.model.as_deref().unwrap_or("opus");
    let workflow = create_workflow(&args.agent);
    eprintln!("agent: {}", workflow.backend().name());
    eprintln!("model: {}", model);

    let inherit_stdin = io::stdin().is_terminal();

    let plan_dir = if let Some(ref plan_dir) = args.plan_dir {
        plan_dir.clone()
    } else if let Some(resumable) = find_resumable_plan_dir(&args.output_dir) {
        if let Ok(cs) = read_changeset(&resumable) {
            let state = cs.state.current.as_str();
            if next_goal_for_state(state).is_none() {
                eprintln!(
                    "Workflow already complete (state: {}). Nothing to do.",
                    state
                );
                return Ok(());
            }
        }
        resumable
    } else {
        let mut input = read_feature_input(args).context("read feature description")?;
        input = input.trim().to_string();
        if input.is_empty() {
            anyhow::bail!("empty feature description");
        }
        let mut workflow = create_workflow(&args.agent);
        let plan_options = PlanOptions {
            model: args.model.clone(),
            agent_output: args.agent_output,
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
                    answers = Some(read_answers(&questions).context("read answers")?);
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
            agent_output: args.agent_output,
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
                    answers = Some(read_answers(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if run_red {
        let red_options = RedOptions {
            model: args.model.clone(),
            agent_output: args.agent_output,
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
                    answers = Some(read_answers(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    let green_options = GreenOptions {
        model: args.model.clone(),
        agent_output: args.agent_output,
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
                return Ok(());
            }
            Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                answers = Some(read_answers(&questions).context("read answers")?);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Read feature description. Uses --prompt if set; otherwise stdin (interactive or piped).
fn read_feature_input(args: &Args) -> anyhow::Result<String> {
    if let Some(ref p) = args.prompt {
        return Ok(p.clone());
    }
    let stdin = io::stdin();
    if stdin.is_terminal() {
        Text::new("Feature description:")
            .with_help_message("Describe what you want to build (e.g. 'Build a user auth system')")
            .prompt()
            .map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        let mut buf = String::new();
        stdin.lock().read_to_string(&mut buf)?;
        Ok(buf)
    }
}

/// Strip leading "N. " or "N) " from a question if present (LLM often numbers them).
fn strip_leading_number(s: &str) -> String {
    let s = s.trim();
    let rest = s.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest != s
        && (rest.starts_with(". ")
            || rest.starts_with(") ")
            || rest.starts_with('.')
            || rest.starts_with(')'))
    {
        let rest = rest.trim_start_matches(['.', ')', ' ']);
        return rest.to_string();
    }
    s.to_string()
}

/// Read answers to clarification questions. When interactive (TTY), uses inquire.
/// Uses Select/MultiSelect for option-based questions, Text for free-form.
/// When piped, reads line by line until EOF.
fn read_answers(questions: &[ClarificationQuestion]) -> anyhow::Result<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        let mut answers = Vec::with_capacity(questions.len());
        for q in questions {
            let prompt = strip_leading_number(&q.question);
            let answer = if q.options.is_empty() {
                Text::new(&prompt)
                    .with_help_message("Press Enter to submit")
                    .prompt()
                    .map_err(|e| anyhow::anyhow!("{}", e))?
            } else if q.multi_select {
                let options: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
                let chosen = MultiSelect::new(&prompt, options)
                    .with_help_message("Space to select, Enter to confirm")
                    .prompt()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                chosen.join(", ")
            } else {
                let options: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
                Select::new(&prompt, options)
                    .prompt()
                    .map_err(|e| anyhow::anyhow!("{}", e))?
                    .to_string()
            };
            answers.push(answer);
        }
        Ok(answers.join("\n"))
    } else {
        println!("\nClarification needed:");
        for (i, q) in questions.iter().enumerate() {
            let prompt = strip_leading_number(&q.question);
            println!("  {}. {}", i + 1, prompt);
        }
        println!("\nEnter answers (one per line):");
        let mut lines = Vec::with_capacity(questions.len());
        let mut lock = stdin.lock();
        let mut buf = String::new();
        for _ in questions {
            buf.clear();
            let n = lock.read_line(&mut buf)?;
            if n == 0 {
                break;
            }
            let line = buf.trim_end_matches('\n').trim_end_matches('\r');
            lines.push(line.to_string());
        }
        Ok(lines.join("\n"))
    }
}
