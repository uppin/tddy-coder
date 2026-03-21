//! Plain mode: piped stdin, no TUI. Used when stdin or stderr is not a TTY.

use std::io::{self, BufRead, Write};

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

/// Map 1-based menu index to the option label at that position (clamped).
#[must_use]
pub fn resolve_backend_selection_index(index: usize, question: &ClarificationQuestion) -> String {
    if question.options.is_empty() {
        return String::new();
    }
    let idx = index
        .saturating_sub(1)
        .min(question.options.len().saturating_sub(1));
    question.options[idx].label.clone()
}

/// Print backend menu to stderr and read a 1-based choice from stdin. Returns selected option label.
pub fn read_backend_selection_plain(question: &ClarificationQuestion) -> anyhow::Result<String> {
    eprintln!("\n{}: {}", question.header, question.question);
    for (i, opt) in question.options.iter().enumerate() {
        eprintln!("  {}. {} — {}", i + 1, opt.label, opt.description);
    }
    eprint!("Select backend [1-{}]: ", question.options.len().max(1));
    let _ = io::stderr().flush();
    let mut buf = String::new();
    io::stdin().lock().read_line(&mut buf)?;
    let choice = buf.trim().parse::<usize>().unwrap_or(1);
    Ok(resolve_backend_selection_index(choice, question))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::backend::backend_selection_question;

    #[test]
    fn resolve_backend_selection_index_claude() {
        let q = backend_selection_question();
        assert_eq!(resolve_backend_selection_index(1, &q), "Claude");
    }

    #[test]
    fn resolve_backend_selection_index_cursor() {
        let q = backend_selection_question();
        assert_eq!(resolve_backend_selection_index(3, &q), "Cursor");
    }

    #[test]
    fn resolve_backend_selection_index_out_of_bounds_defaults_to_first() {
        let q = backend_selection_question();
        assert_eq!(resolve_backend_selection_index(99, &q), "Stub");
    }
}
