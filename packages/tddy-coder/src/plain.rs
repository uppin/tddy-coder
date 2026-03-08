//! Plain mode: piped stdin, no TUI. Used when stdin or stderr is not a TTY.

use std::io::{self, BufRead};

use tddy_core::ClarificationQuestion;

/// Strip leading "N. " or "N) " from a question if present (LLM often numbers them).
pub fn strip_leading_number(s: &str) -> String {
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

/// Read answers to clarification questions from stdin (one per line).
/// Used in piped mode when no TUI is available.
pub fn read_answers_plain(questions: &[ClarificationQuestion]) -> anyhow::Result<String> {
    println!("\nClarification needed:");
    for (i, q) in questions.iter().enumerate() {
        let prompt = strip_leading_number(&q.question);
        println!("  {}. {}", i + 1, prompt);
    }
    println!("\nEnter answers (one per line):");
    let mut lines = Vec::with_capacity(questions.len());
    let mut lock = io::stdin().lock();
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

/// Prompt user to Run or Skip the demo. Returns true for Run, false for Skip.
/// Used in plain mode when demo-plan.md exists after the green goal.
pub fn read_demo_choice_plain() -> anyhow::Result<bool> {
    println!("\nRun demo? [r] Run  [s] Skip: ");
    let mut buf = String::new();
    io::stdin().lock().read_line(&mut buf)?;
    let choice = buf.trim().trim_end_matches('\r');
    Ok(choice.eq_ignore_ascii_case("r") || choice.eq_ignore_ascii_case("run"))
}
