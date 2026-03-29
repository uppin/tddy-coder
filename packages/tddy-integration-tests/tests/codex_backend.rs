//! Acceptance tests for CodexBackend (`codex exec --json`, `codex exec resume <id>`).
//!
//! Uses a shell stub that records argv and emits fixture JSONL, mirroring `cursor_backend.rs`.

mod common;

use std::fs;

use common::stub_invoke_request;
use std::path::PathBuf;
use tddy_core::backend::SessionMode;
use tddy_core::{BackendError, CodexBackend, CodingBackend};

/// CodexBackend spawns `codex` with `exec`, `--json`, and the merged prompt.
#[test]
#[cfg(unix)]
fn codex_backend_spawns_exec_with_json_and_prompt() {
    let tmp = std::env::temp_dir().join("tddy-codex-backend-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"session","session_id":"codex-sess-1"}}'
printf '%s\n' '{{"type":"item.completed","item":{{"text":"parsed output"}}}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("codex");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CodexBackend::with_path(script_path);
    let req = stub_invoke_request("test prompt", "plan");

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "parsed output");
    assert_eq!(result.session_id.as_deref(), Some("codex-sess-1"));

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    assert_eq!(
        captured.lines().next(),
        Some("exec"),
        "first argv after binary should be exec, got: {}",
        captured
    );
    assert!(
        captured.lines().any(|l| l == "--json"),
        "should pass --json, got: {}",
        captured
    );
    assert!(
        captured.lines().any(|l| l == "test prompt"),
        "should include user prompt, got: {}",
        captured
    );
    assert_eq!(backend.name(), "codex");
}

/// Resume uses `exec`, `resume`, then session id (Codex CLI subcommand shape).
#[test]
#[cfg(unix)]
fn codex_backend_resume_subcommand_includes_session_id() {
    let tmp = std::env::temp_dir().join("tddy-codex-resume-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"item.completed","item":{{"text":"ok"}}}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("codex");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CodexBackend::with_path(script_path);
    let mut req = stub_invoke_request("continue", "plan");
    req.session = Some(SessionMode::Resume("prev-codex-session".to_string()));

    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    let lines: Vec<&str> = captured.lines().collect();
    let exec_pos = lines.iter().position(|l| *l == "exec").expect("exec");
    assert_eq!(lines.get(exec_pos + 1).copied(), Some("resume"));
    assert_eq!(lines.get(exec_pos + 2).copied(), Some("prev-codex-session"));
}

#[test]
#[cfg(unix)]
fn codex_backend_includes_model_flag_when_set() {
    let tmp = std::env::temp_dir().join("tddy-codex-model-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"item.completed","item":{{"text":"x"}}}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("codex");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CodexBackend::with_path(script_path);
    let mut req = stub_invoke_request("hi", "red");
    req.model = Some("gpt-5".to_string());

    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    let lines: Vec<&str> = captured.lines().collect();
    let mpos = lines.iter().position(|l| *l == "-m").expect("-m flag");
    assert_eq!(lines.get(mpos + 1).copied(), Some("gpt-5"));
}

/// System prompt is merged into the argv/stdin payload like CursorBackend (file wins over inline).
#[test]
#[cfg(unix)]
fn codex_backend_merges_system_prompt_like_cursor() {
    let tmp = std::env::temp_dir().join("tddy-codex-sysprompt-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"item.completed","item":{{"text":"ok"}}}}'
exit 0
"##,
        args_file.display()
    );
    let script_path = tmp.join("codex");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CodexBackend::with_path(script_path);
    let mut req = stub_invoke_request("user task text", "plan");
    req.system_prompt = Some("SYSTEM_BLOCK_A".to_string());

    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");

    let captured = fs::read_to_string(&args_file).expect("read captured args");
    // Merged prompt uses newlines like CursorBackend; printf records it as multiple lines in the capture file.
    assert!(
        captured.contains("SYSTEM_BLOCK_A") && captured.contains("user task text"),
        "merged argv should include system then user like Cursor, got:\n{}",
        captured
    );
}

#[test]
#[cfg(unix)]
fn codex_backend_propagates_exit_code() {
    let tmp = std::env::temp_dir().join("tddy-codex-exit-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"item.completed","item":{{"text":"partial"}}}}'
exit 7
"##,
        args_file.display()
    );
    let script_path = tmp.join("codex");
    fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let backend = CodexBackend::with_path(script_path);
    let req = stub_invoke_request("x", "green");

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should return Ok with stderr/output populated");
    assert_eq!(result.exit_code, 7);
}

#[test]
#[cfg(unix)]
fn codex_backend_reports_binary_not_found() {
    let missing = tmp_abs_join_codex_missing();
    let backend = CodexBackend::with_path(missing);
    let req = stub_invoke_request("noop", "plan");

    let err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect_err("expected BinaryNotFound for missing codex binary");

    match err {
        BackendError::BinaryNotFound(msg) => {
            assert!(
                msg.contains("codex") || msg.to_lowercase().contains("not found"),
                "message should name the binary or not-found: {}",
                msg
            );
        }
        other => panic!("expected BinaryNotFound, got {:?}", other),
    }
}

fn tmp_abs_join_codex_missing() -> PathBuf {
    let tmp = std::env::temp_dir().join("tddy-codex-missing-bin");
    let _ = std::fs::create_dir_all(&tmp);
    tmp.canonicalize()
        .unwrap_or(tmp)
        .join("definitely-no-codex-here-9f3a2c1b")
}
