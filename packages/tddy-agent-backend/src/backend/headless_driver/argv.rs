//! The argv one turn runs its CLI with: the harness's command, the session to resume (`None` on a
//! conversation's first turn), and the prompt. Each CLI's working directory is applied by the
//! spawn; codex and oz also take it as a flag.

use std::path::Path;

use super::{HeadlessCommand, HeadlessHarness};

pub(super) fn turn_argv(
    harness: HeadlessHarness,
    command: &HeadlessCommand,
    session: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    match harness {
        HeadlessHarness::Claude => claude(command, session, prompt),
        HeadlessHarness::Cursor => cursor(command, session, prompt),
        HeadlessHarness::Codex => codex(command, session, prompt),
        HeadlessHarness::Oz => oz(command, session, prompt),
    }
}

fn claude(command: &HeadlessCommand, session: Option<&str>, prompt: &str) -> Vec<String> {
    let mut argv = words(&["-p", prompt, "--output-format", "stream-json", "--verbose"]);
    push_flag(&mut argv, "--model", command.model.as_deref());
    push_flag(&mut argv, "--resume", session);
    push_flag(
        &mut argv,
        "--permission-prompt-tool",
        command.permission_prompt_tool.as_deref(),
    );
    let mcp_config = path_text(command.mcp_config.as_deref());
    push_flag(&mut argv, "--mcp-config", mcp_config.as_deref());
    for tool in &command.disallowed_tools {
        push_flag(&mut argv, "--disallowedTools", Some(tool.as_str()));
    }
    argv
}

/// Cursor runs read-only unless the operator's profile opts into `--force`; no such field exists on
/// [`HeadlessCommand`], so it is never passed here.
fn cursor(command: &HeadlessCommand, session: Option<&str>, prompt: &str) -> Vec<String> {
    let mut argv = Vec::new();
    push_flag(&mut argv, "--resume", session);
    argv.extend(words(&["-p", prompt]));
    push_flag(&mut argv, "--model", command.model.as_deref());
    argv.extend(words(&[
        "--output-format",
        "stream-json",
        "--stream-partial-output",
        "--trust",
        "--approve-mcps",
    ]));
    argv
}

fn codex(command: &HeadlessCommand, session: Option<&str>, prompt: &str) -> Vec<String> {
    let mut argv = words(&["exec"]);
    let cwd = path_text(command.cwd.as_deref());
    push_flag(&mut argv, "-C", cwd.as_deref());
    argv.extend(words(&["-s", "workspace-write"]));
    push_flag(&mut argv, "-m", command.model.as_deref());
    argv.push("--json".to_string());
    if let Some(thread) = session {
        argv.extend(words(&["resume", thread]));
    }
    argv.push(prompt.to_string());
    argv
}

fn oz(command: &HeadlessCommand, session: Option<&str>, prompt: &str) -> Vec<String> {
    let mut argv = words(&["agent", "run", "-p", prompt, "--output-format", "ndjson"]);
    let cwd = path_text(command.cwd.as_deref());
    push_flag(&mut argv, "-C", cwd.as_deref());
    push_flag(&mut argv, "--conversation", session);
    push_flag(&mut argv, "--model", command.model.as_deref());
    let mcp_config = path_text(command.mcp_config.as_deref());
    push_flag(&mut argv, "--mcp", mcp_config.as_deref());
    push_flag(&mut argv, "--profile", command.profile.as_deref());
    argv
}

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// `flag value`, when there is a value.
fn push_flag(argv: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        argv.push(flag.to_string());
        argv.push(value.to_string());
    }
}

fn path_text(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(harness: HeadlessHarness, session: Option<&str>) -> Vec<String> {
        turn_argv(harness, &HeadlessCommand::default(), session, "Go")
    }

    #[test]
    fn cursor_never_passes_force() {
        let argv = turn(HeadlessHarness::Cursor, None);
        assert!(!argv.iter().any(|arg| arg == "--force"), "{argv:?}");
        assert!(argv.iter().any(|arg| arg == "--trust"), "{argv:?}");
    }

    #[test]
    fn a_claude_first_turn_names_no_session() {
        let argv = turn(HeadlessHarness::Claude, None);
        assert!(!argv.iter().any(|arg| arg == "--resume"), "{argv:?}");
        assert_eq!(argv[0], "-p");
        assert_eq!(argv[1], "Go");
    }

    #[test]
    fn a_codex_resume_sits_right_before_the_prompt() {
        let argv = turn(HeadlessHarness::Codex, Some("th-1"));
        assert_eq!(argv[argv.len() - 3], "resume");
        assert_eq!(argv[argv.len() - 2], "th-1");
        assert_eq!(argv.last().map(String::as_str), Some("Go"));
    }
}
