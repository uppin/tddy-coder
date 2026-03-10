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

/// Plan approval gate: View (print PRD), Approve (proceed), or Refine (feedback).
/// Returns "Approve" or the refinement feedback string.
/// Used in plain mode after plan completes.
pub fn read_plan_approval_plain(prd_content: &str) -> anyhow::Result<String> {
    loop {
        println!("\nPlan generated. Options: [v] View  [a] Approve  [r] Refine: ");
        let mut buf = String::new();
        let n = io::stdin().lock().read_line(&mut buf)?;
        if n == 0 {
            return Err(anyhow::anyhow!(
                "EOF: no input for plan approval (stdin closed)"
            ));
        }
        let choice = buf.trim().trim_end_matches('\r');
        match choice.to_lowercase().as_str() {
            "v" | "view" => {
                println!("\n--- PRD ---\n{}\n---", prd_content);
            }
            "a" | "approve" => return Ok("Approve".to_string()),
            "r" | "refine" => {
                println!("Enter refinement feedback: ");
                let mut fb = String::new();
                io::stdin().lock().read_line(&mut fb)?;
                return Ok(fb.trim().trim_end_matches('\r').to_string());
            }
            _ => {
                println!("Invalid choice. Use v, a, or r.");
            }
        }
    }
}

/// Prompt user to Create & run or Skip the demo. Returns true for Create & run, false for Skip.
/// Used in plain mode when demo-plan.md exists after the green goal.
pub fn read_demo_choice_plain() -> anyhow::Result<bool> {
    println!("\nCreate & run a demo? [r] Create & run  [s] Skip: ");
    let mut buf = String::new();
    io::stdin().lock().read_line(&mut buf)?;
    let choice = buf.trim().trim_end_matches('\r');
    Ok(choice.eq_ignore_ascii_case("r") || choice.eq_ignore_ascii_case("run"))
}
