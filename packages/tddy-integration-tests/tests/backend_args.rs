//! Integration tests for the Claude Code CLI argv a workflow turn runs with.
//!
//! The argv is rendered from the `claude` spec by the generic `SpecHeadlessBackend` (`#agent-tools
//! 15/19`; the hand-built `build_claude_args` is gone). Each test runs a turn over a stub `claude`
//! that records what it was handed, and pins that the system prompt and user prompt are separate
//! arguments, that the session flags, tools and approval relay are present.

mod common;

use common::{a_spec_backend, in_spec_workspace, spec_argv, spec_workspace_root};
use tddy_core::{CodingBackend, InvokeRequest, SessionMode};

const RESULT_LINE: &str =
    r#"printf '%s\n' '{"type":"result","result":"ok","session_id":"test-session"}'"#;
/// Copies the file `--append-system-prompt-file` names into the workspace: the turn removes it.
const RECORD_SYSTEM_PROMPT_FILE: &str = r#"prev=""
for a in "$@"; do
  [ "$prev" = "--append-system-prompt-file" ] && cat "$a" > "$(dirname "$0")/../system-prompt-seen.txt"
  prev="$a"
done
"#;

fn request_with_both_prompts(system_prompt: &str, user_prompt: &str) -> InvokeRequest {
    let mut req = common::stub_invoke_request(user_prompt, "plan");
    req.system_prompt = Some(system_prompt.to_string());
    req
}

/// Runs `req` over a stub `claude`; returns the argv it saw and the workspace.
fn run_claude(req: InvokeRequest) -> (Vec<String>, tempfile::TempDir) {
    let (backend, dir) = a_spec_backend(
        "claude",
        &format!("{RECORD_SYSTEM_PROMPT_FILE}{RESULT_LINE}"),
    );
    let req = in_spec_workspace(&dir, req);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");
    (spec_argv(&dir), dir)
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn system_prompt_seen(dir: &tempfile::TempDir) -> String {
    std::fs::read_to_string(spec_workspace_root(dir).join("system-prompt-seen.txt"))
        .expect("the stub copied the system prompt file")
}

#[test]
fn claude_turn_includes_output_format_stream_json() {
    // Given
    let req = request_with_both_prompts("Sys", "User");

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert_eq!(
        value_after(&args, "--output-format").as_deref(),
        Some("stream-json")
    );
    assert!(
        args.contains(&"--verbose".to_string()),
        "stream-json with -p requires --verbose"
    );
}

#[test]
fn claude_turn_includes_session_id_on_first_call() {
    // Given
    let mut req = request_with_both_prompts("Sys", "User");
    req.session = Some(SessionMode::Fresh("abc-123".to_string()));

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert_eq!(
        value_after(&args, "--session-id").as_deref(),
        Some("abc-123")
    );
    assert!(!args.contains(&"--resume".to_string()));
}

#[test]
fn claude_turn_includes_resume_on_followup_call() {
    // Given
    let mut req = request_with_both_prompts("Sys", "User");
    req.session = Some(SessionMode::Resume("abc-123".to_string()));

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert_eq!(value_after(&args, "--resume").as_deref(), Some("abc-123"));
    assert!(!args.contains(&"--session-id".to_string()));
}

#[test]
fn user_prompt_follows_print_flag() {
    // Given
    let req = request_with_both_prompts(
        "You are a technical planning assistant.",
        "Create a PRD for: user auth",
    );

    // When
    let (args, _dir) = run_claude(req);

    // Then — per CLI docs (claude -p "query"), the prompt comes immediately after -p
    assert_eq!(args.first().map(String::as_str), Some("-p"));
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("Create a PRD for: user auth")
    );
}

#[test]
fn system_prompt_and_user_prompt_are_separate_arguments() {
    // Given
    let sys = "System instructions here";
    let user = "User query here";
    let req = request_with_both_prompts(sys, user);

    // When
    let (args, dir) = run_claude(req);

    // Then — the system prompt travels as the content of the file `--append-system-prompt-file` names
    assert!(args.contains(&"--append-system-prompt-file".to_string()));
    assert_eq!(system_prompt_seen(&dir), sys);
    assert_eq!(
        args.get(1).map(String::as_str),
        Some(user),
        "user prompt must follow -p"
    );
}

#[test]
fn no_single_arg_contains_both_system_and_user_prompt_content() {
    // Given
    let req = request_with_both_prompts("SYSTEM_MARKER", "USER_MARKER");

    // When
    let (args, _dir) = run_claude(req);

    // Then
    for arg in &args {
        assert!(
            !(arg.contains("SYSTEM_MARKER") && arg.contains("USER_MARKER")),
            "no arg should contain both system and user content: {arg:?}"
        );
    }
}

#[test]
fn append_system_prompt_file_receives_exactly_one_argument() {
    // Given
    let sys = "Single system prompt value";
    let req = request_with_both_prompts(sys, "User prompt");

    // When
    let (args, dir) = run_claude(req);

    // Then
    let flags = args
        .iter()
        .filter(|a| *a == "--append-system-prompt-file")
        .count();
    assert_eq!(flags, 1, "one system prompt file, got {args:?}");
    let file = value_after(&args, "--append-system-prompt-file").expect("a file follows the flag");
    assert!(
        !file.starts_with("--"),
        "a value follows the flag, got {file:?}"
    );
    assert_eq!(system_prompt_seen(&dir), sys);
    assert_eq!(args.get(1).map(String::as_str), Some("User prompt"));
}

#[test]
fn multiline_system_prompt_reaches_the_agent_whole() {
    // Given
    let sys = "Line 1\nLine 2\nLine 3";
    let req = request_with_both_prompts(sys, "Create PRD for feature X");

    // When
    let (args, dir) = run_claude(req);

    // Then
    assert_eq!(system_prompt_seen(&dir), sys);
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("Create PRD for feature X")
    );
}

#[test]
fn request_without_system_prompt_has_user_prompt_after_p() {
    // Given
    let req = common::stub_invoke_request("Just the user prompt", "plan");

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert!(!args.contains(&"--append-system-prompt".to_string()));
    assert!(!args.contains(&"--append-system-prompt-file".to_string()));
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("Just the user prompt")
    );
}

/// A `system_prompt_path` is passed as is (no copy), and beats an inline `system_prompt`.
#[test]
fn system_prompt_passed_via_file_when_path_provided() {
    // Given
    let path_dir = tempfile::tempdir().expect("dir");
    let path = std::fs::canonicalize(path_dir.path())
        .expect("canonical")
        .join("sys-prompt.txt");
    std::fs::write(&path, "From the file.").expect("write");
    let mut req = request_with_both_prompts("Inline, ignored when a path is set.", "User prompt");
    req.system_prompt_path = Some(path.clone());

    // When
    let (args, dir) = run_claude(req);

    // Then
    assert_eq!(
        value_after(&args, "--append-system-prompt-file").as_deref(),
        Some(path.to_str().unwrap())
    );
    assert!(
        !args.contains(&"--append-system-prompt".to_string()),
        "there is no inline --append-system-prompt any more"
    );
    assert_eq!(system_prompt_seen(&dir), "From the file.");
}

/// The temporary system prompt file an inline prompt is written to does not outlive the turn.
#[test]
fn inline_system_prompt_file_is_removed_after_the_turn() {
    // Given
    let req = request_with_both_prompts("Inline system prompt", "User prompt");

    // When
    let (args, _dir) = run_claude(req);

    // Then
    let file = value_after(&args, "--append-system-prompt-file").expect("file flag");
    assert!(
        !std::path::Path::new(&file).exists(),
        "the temp system prompt file {file} is removed after the turn"
    );
}

/// Each allowed tool is its own `--allowedTools` entry.
#[test]
fn claude_turn_includes_allowed_tools_when_set() {
    // Given
    let mut req = request_with_both_prompts("Sys", "User");
    req.hints.allowed_tools = vec![
        "Read".to_string(),
        "Write".to_string(),
        "Bash(cargo *)".to_string(),
    ];

    // When
    let (args, _dir) = run_claude(req);

    // Then
    let allowed: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--allowedTools")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(allowed, vec!["Read", "Write", "Bash(cargo *)"]);
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("User"),
        "user prompt must follow -p"
    );
}

/// The approval relay and the MCP config naming the tool server are in the argv; the config is
/// `<session_dir>/claude-mcp-config.json`.
#[test]
fn claude_turn_includes_permission_prompt_tool_and_mcp_config() {
    // Given
    let req = request_with_both_prompts("Sys", "User");

    // When
    let (args, dir) = run_claude(req);

    // Then
    assert_eq!(
        value_after(&args, "--permission-prompt-tool").as_deref(),
        Some("mcp__tddy-tools__approval_prompt")
    );
    let expected = spec_workspace_root(&dir).join("session/claude-mcp-config.json");
    assert_eq!(
        value_after(&args, "--mcp-config").as_deref(),
        Some(expected.to_str().unwrap())
    );
    assert_eq!(
        args.get(1).map(String::as_str),
        Some("User"),
        "user prompt must follow -p"
    );
}

/// Plan goal invocation must include --permission-prompt-tool and --mcp-config so Claude Code can
/// route permission requests to tddy-tools MCP server instead of falling back to interactive
/// approval (which fails in headless mode).
#[test]
fn invoke_plan_goal_includes_permission_prompt_tool_and_mcp_config() {
    // Given
    let mut req = common::stub_invoke_request("User prompt", "plan");
    req.system_prompt = Some("System instructions".to_string());

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert!(
        args.contains(&"--permission-prompt-tool".to_string()),
        "got: {args:?}"
    );
    assert!(args.contains(&"--mcp-config".to_string()), "got: {args:?}");
}

/// Green (and other non-plan workflow goals) register the MCP permission relay even when no
/// toolcall socket is configured, so tool approvals use the same path as Plan.
#[test]
fn invoke_green_goal_without_socket_includes_permission_prompt_and_mcp_config() {
    // Given
    let mut req = common::stub_invoke_request("User prompt", "green");
    req.system_prompt = Some("System instructions".to_string());
    assert!(req.socket_path.is_none());

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert!(
        args.contains(&"--permission-prompt-tool".to_string()),
        "got: {args:?}"
    );
    assert!(args.contains(&"--mcp-config".to_string()), "got: {args:?}");
}

/// AcceptanceTests with a `socket_path` register the relay so Bash/tool approvals reach the
/// Virtual TUI instead of Claude Code UI-only prompts.
#[test]
fn invoke_acceptance_tests_with_socket_includes_permission_prompt_and_mcp_config() {
    // Given
    let mut req = common::stub_invoke_request("User prompt", "acceptance-tests");
    req.system_prompt = Some("System instructions".to_string());
    req.socket_path = Some(std::env::temp_dir().join("tddy-backend-args-toolcall.sock"));

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert!(
        args.contains(&"--permission-prompt-tool".to_string()),
        "got: {args:?}"
    );
    assert!(args.contains(&"--mcp-config".to_string()), "got: {args:?}");
}

/// An inline system prompt is passed as `--append-system-prompt-file` (not inline) to avoid
/// argument length limits and shell parsing issues.
#[test]
fn invoke_uses_system_prompt_file_not_inline() {
    // Given
    let mut req = common::stub_invoke_request("User prompt", "plan");
    req.system_prompt = Some("System instructions".to_string());

    // When
    let (args, _dir) = run_claude(req);

    // Then
    assert!(
        args.contains(&"--append-system-prompt-file".to_string()),
        "got: {args:?}"
    );
    assert!(
        !args.iter().any(|l| l == "--append-system-prompt"),
        "got: {args:?}"
    );
}
