//! Acceptance tests: plain (non-TUI) CLI completion of a single `free-prompting` turn.
//!
//! Feature: docs/ft/coder/1-WIP/PRD-2026-07-01-plain-mode-free-prompting-completion.md
//! Changeset: docs/dev/1-WIP/2026-07-01-plain-mode-free-prompting-completion.md
//!
//! `FreePromptingRecipe`'s single `prompting` task has no graph successor, so a backend that
//! finishes a turn without calling `tddy-tools submit` or asking a clarification question makes
//! `tddy-graph`'s `FlowRunner` synthesize `ExecutionStatus::WaitingForInput` with no
//! `pending_questions` in context. The plain CLI currently treats every `WaitingForInput` as an
//! explicit clarification request and crashes with `Error: no pending questions`.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::fs;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// First line of each phrase `StubBackend::prompting_response` may pick — the exact phrase is
/// chosen at runtime from a nanosecond timestamp, so membership in this fixed, finite set is the
/// most precise assertion available (see `packages/tddy-core/src/backend/stub.rs` `prompting_response`).
const STUB_PROMPTING_PHRASE_OPENERS: &[&str] = &[
    "Great question!",
    "Start simple, then iterate.",
    "Three approaches here.",
    "Data flow drives design.",
    "Every abstraction costs.",
    "Define inputs and outputs.",
    "Separate change from stable.",
    "Prefer composition.",
    "Unix philosophy applies.",
    "Spike it first.",
    "Flip the problem.",
    "Iterate, don't plan forever.",
];

/// A single `--recipe free-prompting` turn with a backend that never submits and never asks a
/// clarification question completes successfully in plain mode, printing the backend's response,
/// instead of crashing with "no pending questions".
#[test]
fn free_prompting_plain_mode_completes_a_single_turn_and_prints_the_response() {
    // Given — an isolated tddy home so this test never touches a real ~/.tddy
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-free-prompting-plain-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tddy_data_dir);
    fs::create_dir_all(&tddy_data_dir).expect("create tddy data dir");

    let mut cmd = tddy_coder_bin();
    cmd.args([
        "--agent",
        "stub",
        "--recipe",
        "free-prompting",
        "--prompt",
        "What is the best way to structure a Rust CLI?",
        "--tddy-data-dir",
        tddy_data_dir.to_str().unwrap(),
    ]);

    // When
    let output = cmd.output().expect("run tddy-coder");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Then — succeeds instead of crashing with "no pending questions"
    assert!(
        output.status.success(),
        "free-prompting single turn should exit 0 in plain mode; stdout={stdout} stderr={stderr}"
    );
    assert!(
        !combined.contains("no pending questions"),
        "must not hit the pending-questions crash; stdout={stdout} stderr={stderr}"
    );
    assert!(
        STUB_PROMPTING_PHRASE_OPENERS
            .iter()
            .any(|opener| combined.contains(opener)),
        "expected the backend's prompting response to be printed; stdout={stdout} stderr={stderr}"
    );

    let _ = fs::remove_dir_all(&tddy_data_dir);
}

/// A goal that genuinely requires clarification (the `tdd` recipe's `plan` goal, via the `stub`
/// backend) is unaffected by the fix above: the plain CLI still reads an answer from stdin and
/// completes successfully.
#[test]
#[cfg(unix)]
fn plan_goal_clarification_still_prompts_for_answers_and_completes_in_plain_mode() {
    // Given — an isolated output dir; prompt omits SKIP_QUESTIONS so stub asks a real question
    let tmp = std::env::temp_dir().join(format!(
        "tddy-plan-clarification-plain-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create tmp");
    let output_dir = tmp.join("plans-root");

    let mut cmd = tddy_coder_bin();
    cmd.args([
        "--agent",
        "stub",
        "--recipe",
        "tdd",
        "--goal",
        "plan",
        "--prompt",
        "Build auth feature",
        "--output-dir",
        output_dir.to_str().unwrap(),
    ])
    // "OAuth" answers the clarification question; "a" approves the plan that follows.
    .write_stdin("OAuth\na\n");

    // When
    let output = cmd.output().expect("run tddy-coder");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Then — still asks for clarification (unaffected by the free-prompting fix) and completes
    assert!(
        stdout.contains("Clarification needed"),
        "plan goal should still prompt for clarification; stdout={stdout} stderr={stderr}"
    );
    assert!(
        output.status.success(),
        "plan with an answered clarification should still succeed; stdout={stdout} stderr={stderr}"
    );

    let _ = fs::remove_dir_all(&tmp);
}
