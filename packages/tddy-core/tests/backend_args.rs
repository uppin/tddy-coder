//! Integration tests for Claude Code CLI argument construction.
//!
//! Verifies that system prompt and user prompt are passed as separate arguments
//! to avoid malformed commands where they appear concatenated.

use std::fs;
use tddy_core::{
    build_claude_args, ClaudeCodeBackend, CodingBackend, InvokeRequest, PermissionMode,
};

fn request_with_both_prompts(system_prompt: &str, user_prompt: &str) -> InvokeRequest {
    InvokeRequest {
        prompt: user_prompt.to_string(),
        system_prompt: Some(system_prompt.to_string()),
        permission_mode: PermissionMode::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        agent_output: false,
        inherit_stdin: false,
    }
}

#[test]
fn build_claude_args_includes_output_format_stream_json() {
    let req = request_with_both_prompts("Sys", "User");
    let args = build_claude_args(&req, None);

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

    let args = build_claude_args(&req, None);

    assert!(args.contains(&"--session-id".to_string()));
    let sid_idx = args.iter().position(|a| a == "--session-id").unwrap();
    assert_eq!(args.get(sid_idx + 1), Some(&"abc-123".to_string()));
}

#[test]
fn build_claude_args_includes_resume_on_followup_call() {
    let mut req = request_with_both_prompts("Sys", "User");
    req.session_id = Some("abc-123".to_string());
    req.is_resume = true;

    let args = build_claude_args(&req, None);

    assert!(args.contains(&"--resume".to_string()));
    let resume_idx = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args.get(resume_idx + 1), Some(&"abc-123".to_string()));
}

#[test]
fn user_prompt_is_last_argument() {
    let req = request_with_both_prompts(
        "You are a technical planning assistant.",
        "Create a PRD for: user auth",
    );
    let args = build_claude_args(&req, None);

    let last = args.last().expect("args should not be empty");
    assert_eq!(last, "Create a PRD for: user auth");
}

#[test]
fn system_prompt_and_user_prompt_are_separate_arguments() {
    let sys = "System instructions here";
    let user = "User query here";
    let req = request_with_both_prompts(sys, user);
    let args = build_claude_args(&req, None);

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
    assert_eq!(args.last().unwrap(), user);
}

#[test]
fn no_single_arg_contains_both_system_and_user_prompt_content() {
    let sys = "SYSTEM_MARKER";
    let user = "USER_MARKER";
    let req = request_with_both_prompts(sys, user);
    let args = build_claude_args(&req, None);

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
    let args = build_claude_args(&req, None);

    let append_idx = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .expect("--append-system-prompt should be present");
    let value_idx = append_idx + 1;
    assert_eq!(args[value_idx], sys);
    assert_eq!(
        args[value_idx + 1],
        user,
        "user prompt should follow system prompt as separate arg"
    );
}

#[test]
fn multiline_system_prompt_passed_as_single_arg() {
    let sys = "Line 1\nLine 2\nLine 3";
    let user = "Create PRD for feature X";
    let req = request_with_both_prompts(sys, user);
    let args = build_claude_args(&req, None);

    let sys_idx = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .unwrap()
        + 1;
    assert_eq!(args[sys_idx], sys);
    assert_eq!(args.last().unwrap(), user);
}

#[test]
fn request_without_system_prompt_has_user_prompt_last() {
    let req = InvokeRequest {
        prompt: "Just the user prompt".to_string(),
        system_prompt: None,
        permission_mode: PermissionMode::Default,
        model: None,
        session_id: None,
        is_resume: false,
        agent_output: false,
        inherit_stdin: false,
    };
    let args = build_claude_args(&req, None);

    assert!(!args.contains(&"--append-system-prompt".to_string()));
    assert!(!args.contains(&"--append-system-prompt-file".to_string()));
    assert_eq!(args.last().unwrap(), "Just the user prompt");
}

/// Uses --append-system-prompt-file (not --append-system-prompt) when path is provided,
/// to avoid argument length limits and parsing issues with newlines/special chars.
#[test]
fn system_prompt_passed_via_file_when_path_provided() {
    let sys = "System instructions with newlines\nand special chars";
    let user = "User prompt";
    let req = request_with_both_prompts(sys, user);
    let path = std::path::Path::new("/tmp/sys-prompt.txt");
    let args = build_claude_args(&req, Some(path));

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
        permission_mode: PermissionMode::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        agent_output: false,
        inherit_stdin: false,
    };

    let _ = backend.invoke(req).expect("invoke should succeed");

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
