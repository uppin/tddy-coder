//! Acceptance tests for CursorBackend.
//!
//! Verifies that CursorBackend spawns `cursor agent` with correct flags,
//! parses Cursor's stream-json output, and captures thread_id for --resume.

use std::fs;
use tddy_core::{CodingBackend, CursorBackend, Goal, InvokeRequest};

/// CursorBackend spawns cursor agent with -p, --output-format stream-json, --force, --trust.
#[test]
fn cursor_backend_spawns_cursor_agent_with_correct_flags() {
    let tmp = std::env::temp_dir().join("tddy-cursor-backend-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
# Emit minimal Cursor stream-json: system event with thread_id, then result
printf '%s\n' '{{"type":"system","thread_id":"cursor-thread-abc"}}'
printf '%s\n' '{{"type":"result","result":"output","session_id":"cursor-thread-abc"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CursorBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "test prompt".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let result = backend.invoke(req).expect("invoke should succeed");
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "output");
    assert_eq!(result.session_id.as_deref(), Some("cursor-thread-abc"));

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    assert!(
        captured.contains("agent"),
        "should have 'agent' subcommand, got: {}",
        captured
    );
    assert!(
        captured.contains("-p"),
        "should have -p flag, got: {}",
        captured
    );
    assert!(
        captured.contains("--output-format"),
        "should have --output-format, got: {}",
        captured
    );
    assert!(
        captured.contains("stream-json"),
        "should have stream-json, got: {}",
        captured
    );
    assert!(
        captured.contains("--force"),
        "should have --force, got: {}",
        captured
    );
    assert!(
        captured.contains("--trust"),
        "should have --trust, got: {}",
        captured
    );
    assert!(
        captured.contains("--plan"),
        "should have --plan when goal is Plan, got: {}",
        captured
    );
}

/// CursorBackend does not pass --plan when goal is not Plan.
#[test]
fn cursor_backend_omits_plan_flag_when_goal_is_not_plan() {
    let tmp = std::env::temp_dir().join("tddy-cursor-no-plan-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"ok","session_id":"s1"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CursorBackend::with_path(script_path.into());
    for goal in [Goal::AcceptanceTests, Goal::Red, Goal::Green] {
        let req = InvokeRequest {
            prompt: "Create tests".to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal,
            model: None,
            session_id: None,
            is_resume: false,
            working_dir: None,
            debug: false,
            agent_output: false,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
        };
        let _ = backend.invoke(req).expect("invoke should succeed");
        let captured = fs::read_to_string(&args_file).expect("read captured args");
        assert!(
            !captured.contains("--plan"),
            "should not have --plan when goal is {:?}, got: {}",
            goal,
            captured
        );
    }
}

/// CursorBackend adds --resume when session_id and is_resume are set.
#[test]
fn cursor_backend_adds_resume_flag_on_followup() {
    let tmp = std::env::temp_dir().join("tddy-cursor-resume-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"continued"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CursorBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "continue".to_string(),
        system_prompt: None,
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: Some("prev-thread-id".to_string()),
        is_resume: true,
        working_dir: None,
        debug: false,
        agent_output: false,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let _ = backend.invoke(req).expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    assert!(
        captured.contains("--resume"),
        "should have --resume on followup, got: {}",
        captured
    );
    assert!(
        captured.lines().any(|l| l == "prev-thread-id"),
        "should pass thread id to --resume, got: {}",
        captured
    );
}

/// CursorBackend returns name "cursor".
#[test]
fn cursor_backend_name_returns_cursor() {
    let backend = CursorBackend::new();
    assert_eq!(backend.name(), "cursor");
}

/// Extract the prompt value passed to -p from captured args (one arg per line).
/// The prompt may span multiple lines when it contains system + user content.
fn prompt_from_captured_args(captured: &str) -> Option<String> {
    let lines: Vec<&str> = captured.lines().collect();
    let i = lines.iter().position(|l| *l == "-p")?;
    let start = i + 1;
    // Prompt runs until the next flag (--output-format)
    let end = lines[start..]
        .iter()
        .position(|l| *l == "--output-format")
        .map(|j| start + j)
        .unwrap_or(lines.len());
    Some(lines[start..end].join("\n"))
}

/// CursorBackend includes system_prompt in the prompt passed to -p when set.
/// Cursor CLI has no --system-prompt; we prepend system instructions to the user prompt.
#[test]
#[cfg(unix)]
fn cursor_backend_includes_system_prompt_in_prompt_when_set() {
    let tmp = std::env::temp_dir().join("tddy-cursor-sysprompt-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"ok","session_id":"s1"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let backend = CursorBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "Create a PRD for: Add login".to_string(),
        system_prompt: Some("You MUST output a <structured-response> block.".to_string()),
        system_prompt_path: None,
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let _ = backend.invoke(req).expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    let prompt = prompt_from_captured_args(&captured).expect("prompt should be present after -p");
    assert!(
        prompt.contains("You MUST output a <structured-response> block."),
        "prompt should include system_prompt content, got: {}",
        prompt
    );
    assert!(
        prompt.contains("Create a PRD for: Add login"),
        "prompt should include user prompt, got: {}",
        prompt
    );
}

/// CursorBackend includes system_prompt_path file content in the prompt when set.
#[test]
#[cfg(unix)]
fn cursor_backend_includes_system_prompt_path_content_in_prompt_when_set() {
    let tmp = std::env::temp_dir().join("tddy-cursor-sysprompt-file-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");
    let system_file = tmp_abs.join("system.md");
    fs::write(&system_file, "Output format: structured-response only.").expect("write system file");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"ok","session_id":"s1"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let backend = CursorBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "Plan: Add logout".to_string(),
        system_prompt: None,
        system_prompt_path: Some(system_file),
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let _ = backend.invoke(req).expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    let prompt = prompt_from_captured_args(&captured).expect("prompt should be present after -p");
    assert!(
        prompt.contains("Output format: structured-response only."),
        "prompt should include system_prompt_path file content, got: {}",
        prompt
    );
    assert!(
        prompt.contains("Plan: Add logout"),
        "prompt should include user prompt, got: {}",
        prompt
    );
}

/// CursorBackend prefers system_prompt_path over system_prompt when both are set.
#[test]
#[cfg(unix)]
fn cursor_backend_prefers_system_prompt_path_over_system_prompt() {
    let tmp = std::env::temp_dir().join("tddy-cursor-sysprompt-prefer-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");
    let system_file = tmp_abs.join("system.md");
    fs::write(&system_file, "From file: use structured-response.").expect("write system file");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"result","result":"ok","session_id":"s1"}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("cursor");
    fs::write(&script_path, script).expect("write script");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    let backend = CursorBackend::with_path(script_path.into());
    let req = InvokeRequest {
        prompt: "Plan feature".to_string(),
        system_prompt: Some("Inline: ignored when path set.".to_string()),
        system_prompt_path: Some(system_file),
        goal: Goal::Plan,
        model: None,
        session_id: None,
        is_resume: false,
        working_dir: None,
        debug: false,
        agent_output: false,
        conversation_output_path: None,
        inherit_stdin: false,
        extra_allowed_tools: None,
    };

    let _ = backend.invoke(req).expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    let prompt = prompt_from_captured_args(&captured).expect("prompt should be present after -p");
    assert!(
        prompt.contains("From file: use structured-response."),
        "prompt should use system_prompt_path content, got: {}",
        prompt
    );
    assert!(
        !prompt.contains("Inline: ignored when path set."),
        "prompt should not use system_prompt when path is set, got: {}",
        prompt
    );
}
