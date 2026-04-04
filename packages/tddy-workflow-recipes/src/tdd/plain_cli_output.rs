//! Plain `--goal` CLI output formatting for the TDD recipe (lives in recipes, not `tddy-coder`).

use std::path::Path;

use tddy_core::GoalId;

use crate::parser::{
    parse_acceptance_tests_response, parse_demo_response, parse_evaluate_response,
    parse_green_response, parse_red_response, parse_refactor_response, parse_update_docs_response,
    parse_validate_subagents_response,
};

/// Print structured goal output for `tddy-coder --goal …` plain mode.
pub fn print_plain_goal_output(
    goal_id: &GoalId,
    output: Option<&str>,
    session_dir: &Path,
) -> Result<(), String> {
    match goal_id.as_str() {
        "interview" => {
            let _ = (output, session_dir);
        }
        "plan" => {
            // Plan goal: print only the path (CLI contract for piping/scripts)
            println!("{}", session_dir.display());
            return Ok(());
        }
        "acceptance-tests" => {
            let out = output
                .and_then(|s| parse_acceptance_tests_response(s).ok())
                .ok_or_else(|| "no parseable acceptance-tests output".to_string())?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "red" => {
            let out = output
                .and_then(|s| parse_red_response(s).ok())
                .ok_or_else(|| "no parseable red output".to_string())?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            for s in &out.skeletons {
                println!(
                    "  [skeleton] {} ({}:{}): {}",
                    s.name,
                    s.file,
                    s.line.unwrap_or(0),
                    s.kind
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "green" => {
            let out = output
                .and_then(|s| parse_green_response(s).ok())
                .ok_or_else(|| "no parseable green output".to_string())?;
            println!("{}", out.summary);
            for t in &out.tests {
                println!(
                    "  - {} ({}:{}): {}",
                    t.name,
                    t.file,
                    t.line.unwrap_or(0),
                    t.status
                );
            }
            for i in &out.implementations {
                println!(
                    "  [impl] {} ({}:{}): {}",
                    i.name,
                    i.file,
                    i.line.unwrap_or(0),
                    i.kind
                );
            }
            if let Some(ref cmd) = out.test_command {
                println!("\nHow to run tests: {}", cmd);
            }
            if let Some(ref prereq) = out.prerequisite_actions {
                println!("Prerequisite actions: {}", prereq);
            }
            if let Some(ref single) = out.run_single_or_selected_tests {
                println!("How to run a single or selected tests: {}", single);
            }
        }
        "evaluate" => {
            let out = output
                .and_then(|s| parse_evaluate_response(s).ok())
                .ok_or_else(|| "no parseable evaluate output".to_string())?;
            println!("{}", out.summary);
            println!("Risk level: {}", out.risk_level);
            println!(
                "Report: {}",
                session_dir.join("evaluation-report.md").display()
            );
        }
        "demo" => {
            let out = output
                .and_then(|s| parse_demo_response(s).ok())
                .ok_or_else(|| "no parseable demo output".to_string())?;
            println!("{}", out.summary);
            println!("Steps completed: {}", out.steps_completed);
        }
        "validate" => {
            let out = output
                .and_then(|s| parse_validate_subagents_response(s).ok())
                .ok_or_else(|| "no parseable validate output".to_string())?;
            println!("{}", out.summary);
        }
        "refactor" => {
            let out = output
                .and_then(|s| parse_refactor_response(s).ok())
                .ok_or_else(|| "no parseable refactor output".to_string())?;
            println!("{}", out.summary);
            println!("Tasks completed: {}", out.tasks_completed);
            println!("Tests passing: {}", out.tests_passing);
        }
        "update-docs" => {
            let out = output
                .and_then(|s| parse_update_docs_response(s).ok())
                .ok_or_else(|| "no parseable update-docs output".to_string())?;
            println!("{}", out.summary);
            println!("Docs updated: {}", out.docs_updated);
        }
        _ => {}
    }
    println!("\nSession dir: {}", session_dir.display());
    Ok(())
}
