//! The argv one turn runs its CLI with: the harness's command, the session to resume (`None` on a
//! conversation's first turn), and the prompt. Each CLI's working directory is applied by the
//! spawn; codex and oz also take it as a flag.

use std::path::Path;

use super::{HeadlessCommand, HeadlessHarness};
use crate::backend::acp_driver::McpServerSpec;

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
    if let Some(server) = &command.mcp_server {
        argv.extend(codex_mcp_server_overrides(server));
    }
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

/// `server` as codex's `-c mcp_servers.<name>.*` config overrides: dotted keys, TOML values. Its
/// environment is named (`env_vars`), never valued: codex forwards those variables from its own
/// environment, so the credential stays off the argv another user's `ps` reads.
fn codex_mcp_server_overrides(server: &McpServerSpec) -> Vec<String> {
    let key = |field: &str| format!("mcp_servers.{}.{field}", server.name);
    let env_names: Vec<&str> = server.env.iter().map(|(name, _)| name.as_str()).collect();
    [
        (
            key("command"),
            toml_value(&server.command.display().to_string()),
        ),
        (key("args"), toml_value(&server.args)),
        (key("env_vars"), toml_value(&env_names)),
    ]
    .into_iter()
    .flat_map(|(key, value)| ["-c".to_string(), format!("{key}={value}")])
    .collect()
}

/// A string or a list of strings as a TOML value. JSON's spelling of both is valid TOML.
fn toml_value(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("strings always serialize as JSON")
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

    /// The `tddy-tools` MCP server a launcher hands a headless conversation, its credential in its
    /// environment.
    fn the_tool_server() -> crate::backend::acp_driver::McpServerSpec {
        crate::backend::acp_driver::McpServerSpec {
            name: "tddy-tools".to_string(),
            command: std::path::PathBuf::from("/opt/tddy/bin/tddy-tools"),
            args: vec!["--mcp".to_string()],
            env: vec![
                (
                    "TDDY_REMOTE_SESSION_TOKEN".to_string(),
                    "minted-agent-token".to_string(),
                ),
                ("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string()),
            ],
        }
    }

    /// The value of each `-c key=value` override in `argv`, by key.
    fn config_overrides(argv: &[String]) -> Vec<(String, String)> {
        argv.windows(2)
            .filter(|pair| pair[0] == "-c")
            .filter_map(|pair| pair[1].split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn codex_is_handed_its_mcp_server_as_config_overrides() {
        // Given
        let command = HeadlessCommand {
            mcp_server: Some(the_tool_server()),
            ..HeadlessCommand::default()
        };

        // When
        let argv = turn_argv(HeadlessHarness::Codex, &command, None, "Go");

        // Then
        let overrides = config_overrides(&argv);
        assert!(
            overrides.contains(&(
                "mcp_servers.tddy-tools.command".to_string(),
                "\"/opt/tddy/bin/tddy-tools\"".to_string()
            )),
            "{argv:?}"
        );
        assert!(
            overrides.contains(&(
                "mcp_servers.tddy-tools.args".to_string(),
                "[\"--mcp\"]".to_string()
            )),
            "{argv:?}"
        );
        assert!(
            overrides.contains(&(
                "mcp_servers.tddy-tools.env_vars".to_string(),
                "[\"TDDY_REMOTE_SESSION_TOKEN\",\"TDDY_CONVERSATION_ID\"]".to_string()
            )),
            "the server's variables are named, so codex forwards them: {argv:?}"
        );
    }

    #[test]
    fn codex_never_carries_its_mcp_servers_credential_on_the_argv() {
        // Given
        let command = HeadlessCommand {
            mcp_server: Some(the_tool_server()),
            ..HeadlessCommand::default()
        };

        // When
        let argv = turn_argv(HeadlessHarness::Codex, &command, None, "Go");

        // Then
        assert!(
            !argv.iter().any(|arg| arg.contains("minted-agent-token")),
            "another user's `ps` reads the argv: {argv:?}"
        );
    }

    #[test]
    fn a_codex_resume_sits_right_before_the_prompt() {
        let argv = turn(HeadlessHarness::Codex, Some("th-1"));
        assert_eq!(argv[argv.len() - 3], "resume");
        assert_eq!(argv[argv.len() - 2], "th-1");
        assert_eq!(argv.last().map(String::as_str), Some("Go"));
    }
}
