//! Integration tests for Claude Code CLI argument construction.
//!
//! Verifies that system prompt and user prompt are passed as separate arguments
//! to avoid malformed commands where they appear concatenated.

use std::fs;
use tddy_core::{
    build_claude_args, plan_allowlist, ClaudeCodeBackend, ClaudeInvokeConfig, CodingBackend, Goal,
    InvokeRequest, PermissionMode,
};

fn plan_config() -> ClaudeInvokeConfig {
    ClaudeInvokeConfig {
        permission_mode: PermissionMode::Plan,
        allowed_tools: plan_allowlist(),
        permission_prompt_tool: None,
        mcp_config_path: None,
    }
}

fn request_with_both_prompts(system_prompt: &str, user_prompt: &str) -> InvokeRequest {
    InvokeRequest {
        prompt: user_prompt.to_string(),
        system_prompt: Some(system_prompt.to_string()),
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    }
}

#[test]
fn build_claude_args_includes_output_format_stream_json() {
    let req = request_with_both_prompts("Sys", "User");
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    assert!(
        args.contains(&"--output-format".to_string()),
        "should include --output-format"
    );
    let of_idx = args.iter().position(|a| a == "--output-format").unwrap();
    assert_eq!(args.get(of_idx + 1), Some(&"stream-json".to_string()));
    assert!(
        args.contains(&"--verbose".to_string()),
        "stream-json with -p requires --verbose"
    );
}

#[test]
fn build_claude_args_includes_session_id_on_first_call() {
    let mut req = request_with_both_prompts("Sys", "User");
    req.session_id = Some("abc-123".to_string());
    req.is_resume = false;

    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    assert!(args.contains(&"--session-id".to_string()));
    let sid_idx = args.iter().position(|a| a == "--session-id").unwrap();
    assert_eq!(args.get(sid_idx + 1), Some(&"abc-123".to_string()));
}

#[test]
fn build_claude_args_includes_resume_on_followup_call() {
    let mut req = request_with_both_prompts("Sys", "User");
    req.session_id = Some("abc-123".to_string());
    req.is_resume = true;

    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    assert!(args.contains(&"--resume".to_string()));
    let resume_idx = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args.get(resume_idx + 1), Some(&"abc-123".to_string()));
}

#[test]
fn user_prompt_follows_print_flag() {
    let req = request_with_both_prompts(
        "You are a technical planning assistant.",
        "Create a PRD for: user auth",
    );
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    // Per CLI docs (claude -p "query"), prompt must come immediately after -p
    let p_idx = args
        .iter()
        .position(|a| a == "-p")
        .expect("-p should be present");
    assert_eq!(
        args.get(p_idx + 1),
        Some(&"Create a PRD for: user auth".to_string())
    );
}

#[test]
fn system_prompt_and_user_prompt_are_separate_arguments() {
    let sys = "System instructions here";
    let user = "User query here";
    let req = request_with_both_prompts(sys, user);
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    let sys_idx = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .expect("--append-system-prompt should be present");
    let sys_value_idx = sys_idx + 1;
    assert!(
        sys_value_idx < args.len(),
        "value for --append-system-prompt should exist"
    );

    assert_eq!(args[sys_value_idx], sys);
    assert_eq!(
        args.get(1).map(|s| s.as_str()),
        Some(user),
        "user prompt must follow -p"
    );
}

#[test]
fn no_single_arg_contains_both_system_and_user_prompt_content() {
    let sys = "SYSTEM_MARKER";
    let user = "USER_MARKER";
    let req = request_with_both_prompts(sys, user);
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    for arg in &args {
        let has_sys = arg.contains("SYSTEM_MARKER");
        let has_user = arg.contains("USER_MARKER");
        assert!(
            !(has_sys && has_user),
            "no arg should contain both system and user content: {:?}",
            arg
        );
    }
}

#[test]
fn append_system_prompt_receives_exactly_one_argument() {
    let sys = "Single system prompt value";
    let user = "User prompt";
    let req = request_with_both_prompts(sys, user);
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    let append_idx = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .expect("--append-system-prompt should be present");
    let value_idx = append_idx + 1;
    assert_eq!(args[value_idx], sys);
    assert_eq!(
        args.get(1).map(|s| s.as_str()),
        Some(user),
        "user prompt must follow -p at args[1]"
    );
}

#[test]
fn multiline_system_prompt_passed_as_single_arg() {
    let sys = "Line 1\nLine 2\nLine 3";
    let user = "Create PRD for feature X";
    let req = request_with_both_prompts(sys, user);
    let config = plan_config();
    let args = build_claude_args(&req, &config, None);

    let sys_idx = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .unwrap()
        + 1;
    assert_eq!(args[sys_idx], sys);
    assert_eq!(
        args.get(1).map(|s| s.as_str()),
        Some(user),
        "user prompt must follow -p"
    );
}

#[test]
fn request_without_system_prompt_has_user_prompt_after_p() {
    let req = InvokeRequest {
        prompt: "Just the user prompt".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };
    let config = ClaudeInvokeConfig {
        permission_mode: PermissionMode::Default,
        allowed_tools: vec![],
        permission_prompt_tool: None,
        mcp_config_path: None,
    };
    let args = build_claude_args(&req, &config, None);

    assert!(!args.contains(&"--append-system-prompt".to_string()));
    assert!(!args.contains(&"--append-system-prompt-file".to_string()));
    assert_eq!(args.get(1), Some(&"Just the user prompt".to_string()));
}

/// Uses --append-system-prompt-file (not --append-system-prompt) when path is provided,
/// to avoid argument length limits and parsing issues with newlines/special chars.
#[test]
fn system_prompt_passed_via_file_when_path_provided() {
    let sys = "System instructions with newlines\nand special chars";
    let user = "User prompt";
    let req = request_with_both_prompts(sys, user);
    let config = plan_config();
    let path = std::path::Path::new("/tmp/sys-prompt.txt");
    let args = build_claude_args(&req, &config, Some(path));

    assert!(
        args.contains(&"--append-system-prompt-file".to_string()),
        "should use --append-system-prompt-file when path provided"
    );
    assert!(
        args.contains(&"/tmp/sys-prompt.txt".to_string()),
        "should pass the path as argument"
    );
    assert!(
        !args.contains(&"--append-system-prompt".to_string()),
        "should not use inline --append-system-prompt when path provided"
    );
}

/// build_claude_args includes --allowedTools for each entry when config.allowed_tools is set.
#[test]
fn build_claude_args_includes_allowed_tools_when_set() {
    let req = request_with_both_prompts("Sys", "User");
    let mut config = plan_config();
    config.allowed_tools = vec![
        "Read".to_string(),
        "Write".to_string(),
        "Bash(cargo *)".to_string(),
    ];

    let args = build_claude_args(&req, &config, None);

    let allowed_tools_indices: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "--allowedTools")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        allowed_tools_indices.len(),
        3,
        "should have three --allowedTools entries"
    );
    assert_eq!(
        args.get(allowed_tools_indices[0] + 1),
        Some(&"Read".to_string())
    );
    assert_eq!(
        args.get(allowed_tools_indices[1] + 1),
        Some(&"Write".to_string())
    );
    assert_eq!(
        args.get(allowed_tools_indices[2] + 1),
        Some(&"Bash(cargo *)".to_string())
    );
    assert_eq!(
        args.get(1),
        Some(&"User".to_string()),
        "user prompt must follow -p"
    );
}

/// build_claude_args includes --permission-prompt-tool and --mcp-config when both are set.
#[test]
fn build_claude_args_includes_permission_prompt_tool_and_mcp_config_when_set() {
    let req = request_with_both_prompts("Sys", "User");
    let mut config = plan_config();
    config.permission_prompt_tool = Some("approval_prompt".to_string());
    config.mcp_config_path = Some(std::path::PathBuf::from("/tmp/mcp.json"));

    let args = build_claude_args(&req, &config, None);

    assert!(
        args.contains(&"--permission-prompt-tool".to_string()),
        "should include --permission-prompt-tool"
    );
    let tool_idx = args
        .iter()
        .position(|a| a == "--permission-prompt-tool")
        .unwrap();
    assert_eq!(args.get(tool_idx + 1), Some(&"approval_prompt".to_string()));

    assert!(
        args.contains(&"--mcp-config".to_string()),
        "should include --mcp-config"
    );
    let mcp_idx = args.iter().position(|a| a == "--mcp-config").unwrap();
    assert_eq!(
        args.get(mcp_idx + 1),
        Some(&"/tmp/mcp.json".to_string()),
        "mcp-config path should be passed"
    );
    assert_eq!(
        args.get(1),
        Some(&"User".to_string()),
        "user prompt must follow -p"
    );
}

/// Invoke uses --append-system-prompt-file (not inline) when system prompt is present,
/// to avoid argument length limits and shell parsing issues.
#[test]
fn invoke_uses_system_prompt_file_not_inline() {
    let tmp = std::env::temp_dir().join("tddy-backend-args-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"---PRD_START---\n# PRD\n---PRD_END---\n---TODO_START---\n- [ ] Task\n---TODO_END---","session_id":"test-session"}}'
"##,
        args_file.display()
    );
    let script_path = tmp.join("claude");
    fs::write(&script_path, script).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = ClaudeCodeBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "User prompt".to_string(),
        system_prompt: Some("System instructions".to_string()),
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        agent_output_sink: None,
        progress_sink: None,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    assert!(
        captured.contains("--append-system-prompt-file"),
        "invoke should use --append-system-prompt-file (not inline), got: {}",
        captured
    );
    assert!(
        !captured.lines().any(|l| l == "--append-system-prompt"),
        "invoke should not use inline --append-system-prompt (only --append-system-prompt-file), got: {}",
        captured
    );
}
