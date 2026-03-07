//! tddy-coder CLI binary.

use anyhow::Context;
use clap::Parser;
use inquire::Text;
use std::io::{self, BufRead, IsTerminal, Read};
use std::path::PathBuf;
use tddy_core::{ClaudeCodeBackend, Workflow, WorkflowError};

#[derive(Parser, Debug)]
#[command(name = "tddy-coder")]
#[command(about = "TDD-driven coder for PRD-based development workflow")]
struct Args {
    /// Goal to execute: plan, develop, etc.
    #[arg(long, value_parser = ["plan"])]
    goal: String,

    /// Output directory for planning artifacts (default: current directory)
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    /// Model name for Claude Code CLI (e.g. sonnet)
    #[arg(short, long)]
    model: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.goal != "plan" {
        anyhow::bail!("unsupported goal: {}", args.goal);
    }

    let mut input = read_feature_input().context("read feature from stdin")?;
    input = input.trim().to_string();
    if input.is_empty() {
        anyhow::bail!("empty feature description (read from stdin)");
    }

    let backend = ClaudeCodeBackend::new();
    let mut workflow = Workflow::new(backend);

    let mut answers: Option<String> = None;
    loop {
        let result = workflow.plan(
            &input,
            &args.output_dir,
            answers.as_deref(),
            args.model.clone(),
        );

        match result {
            Ok(output_path) => {
                println!("Planning complete. Output: {}", output_path.display());
                return Ok(());
            }
            Err(WorkflowError::ClarificationNeeded { questions }) => {
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
        let rest = rest.trim_start_matches(|c: char| c == '.' || c == ')' || c == ' ');
        return rest.to_string();
    }
    s.to_string()
}

/// Read answers to clarification questions. When interactive (TTY), uses inquire.
/// When piped, reads line by line until EOF.
fn read_answers(questions: &[String]) -> anyhow::Result<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        let mut answers = Vec::with_capacity(questions.len());
        for q in questions {
            let prompt = strip_leading_number(q);
            let answer = Text::new(&prompt)
                .with_help_message("Press Enter to submit")
                .prompt()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            answers.push(answer);
        }
        Ok(answers.join("\n"))
    } else {
        println!("\nClarification needed:");
        for (i, q) in questions.iter().enumerate() {
            let q = strip_leading_number(q);
            println!("  {}. {}", i + 1, q);
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

