//! tddy-coder CLI binary.

use anyhow::Context;
use clap::Parser;
use inquire::{MultiSelect, Select, Text};
use std::io::{self, BufRead, IsTerminal, Read};
use std::path::PathBuf;
use tddy_core::{
    AcceptanceTestsOptions, ClarificationQuestion, ClaudeCodeBackend, PlanOptions, ProgressEvent,
    Workflow, WorkflowError,
};

#[derive(Parser, Debug)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
struct Args {
    /// Goal to execute: plan, acceptance-tests, etc.
    #[arg(long, value_parser = ["plan", "acceptance-tests"])]
    goal: String,

    /// Output directory for planning artifacts (default: current directory)
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    /// Plan directory for acceptance-tests goal (required when goal is acceptance-tests)
    #[arg(long)]
    plan_dir: Option<PathBuf>,

    /// Print raw agent output to stderr in real-time
    #[arg(long)]
    agent_output: bool,

    /// Model name for Claude Code CLI (e.g. sonnet)
    #[arg(short, long)]
    model: Option<String>,

    /// Extra tools to add to the goal's allowlist (comma-separated, e.g. "Bash(npm install)")
    #[arg(long, value_delimiter = ',')]
    allowed_tools: Option<Vec<String>>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.goal == "acceptance-tests" {
        let plan_dir = args
            .plan_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("--plan-dir is required for acceptance-tests goal"))?;

        let backend = ClaudeCodeBackend::new().with_progress(|event| {
            let dim = "\x1b[2m";
            let reset = "\x1b[0m";
            match event {
                ProgressEvent::ToolUse {
                    name,
                    detail: Some(d),
                } => eprintln!("  {}📎 {} {}...{}", dim, name, d, reset),
                ProgressEvent::ToolUse { name, detail: None } => {
                    eprintln!("  {}📎 {}...{}", dim, name, reset)
                }
                ProgressEvent::TaskStarted { description } => {
                    eprintln!("  {}▶ {}...{}", dim, description, reset)
                }
                ProgressEvent::TaskProgress {
                    description,
                    last_tool: Some(tool),
                } => eprintln!("  {}⏳ {} ({}){}", dim, description, tool, reset),
                ProgressEvent::TaskProgress {
                    description,
                    last_tool: None,
                } => eprintln!("  {}⏳ {}...{}", dim, description, reset),
            }
        });
        let mut workflow = Workflow::new(backend);

        let inherit_stdin = io::stdin().is_terminal();
        let mut answers: Option<String> = None;
        loop {
            let options = AcceptanceTestsOptions {
                model: args.model.clone(),
                agent_output: args.agent_output,
                inherit_stdin,
                allowed_tools_extras: args.allowed_tools.clone(),
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
                    return Ok(());
                }
                Err(WorkflowError::ClarificationNeeded { questions, .. }) => {
                    answers = Some(read_answers(&questions).context("read answers")?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    if args.goal != "plan" {
        anyhow::bail!("unsupported goal: {}", args.goal);
    }

    let mut input = read_feature_input().context("read feature from stdin")?;
    input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description (read from stdin)");
    }

    let backend = ClaudeCodeBackend::new().with_progress(|event| {
        let dim = "\x1b[2m";
        let reset = "\x1b[0m";
        match event {
            ProgressEvent::ToolUse {
                name,
                detail: Some(d),
            } => eprintln!("  {}📎 {} {}...{}", dim, name, d, reset),
            ProgressEvent::ToolUse { name, detail: None } => {
                eprintln!("  {}📎 {}...{}", dim, name, reset)
            }
            ProgressEvent::TaskStarted { description } => {
                eprintln!("  {}▶ {}...{}", dim, description, reset)
            }
            ProgressEvent::TaskProgress {
                description,
                last_tool: Some(tool),
            } => eprintln!("  {}⏳ {} ({}){}", dim, description, tool, reset),
            ProgressEvent::TaskProgress {
                description,
                last_tool: None,
            } => eprintln!("  {}⏳ {}...{}", dim, description, reset),
        }
    });
    let mut workflow = Workflow::new(backend);

    let inherit_stdin = io::stdin().is_terminal();
    let mut answers: Option<String> = None;
    loop {
        let options = PlanOptions {
            model: args.model.clone(),
            agent_output: args.agent_output,
            inherit_stdin,
            allowed_tools_extras: args.allowed_tools.clone(),
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

/// Read feature description from stdin. When interactive (TTY), uses inquire.
/// When piped, reads until EOF.
fn read_feature_input() -> anyhow::Result<String> {
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
