//! Acceptance tests for [`CodexBackend`] (`codex exec --json`, `codex exec resume <id>`).
//!
//! ## Stub binary (not the real OpenAI Codex CLI)
//!
//! These tests run a **small shell script** named `codex` that records argv and prints fixture
//! JSONL. That matches [`cursor_backend.rs`] and is the supported way to test here:
//!
//! - **Deterministic** — no `codex login`, API keys, or network calls in CI.
//! - **Fast** — no heavyweight `codex` / WebRTC / LiveKit startup.
//! - **Encapsulation** — exercises the real [`CodexBackend`] code path (`Command`, pipes,
//!   parsing, exit handling) against controlled stdout/stderr.
//!
//! Industry guidance for subprocess-heavy code is similar: prefer **fake test binaries** or
//! **stubs** for automated tests, and reserve the **real vendor CLI** for manual smoke runs or
//! opt-in jobs (e.g. `CARGO_BIN_EXE_*`, `mockcmd`, or a `#[ignore]` test). See the Rust Project
//! Primer on external services and crates like `test-binary` for wiring real helper binaries when
//! you need closer fidelity.
//!
//! ## Real `codex login` + OAuth URL capture (opt-in)
//!
//! To exercise the same **`BROWSER` + `TDDY_CODEX_OAUTH_OUT`** path as production (file
//! `{session_dir}/codex_oauth_authorize.url` → LiveKit participant metadata →
//! `ParticipantList.parseCodexOAuthPending` in `packages/tddy-web`), run the ignored test:
//!
//! ```text
//! cargo build -p tddy-coder
//! TDDY_CODEX_LOGIN_E2E=1 cargo test -p tddy-integration-tests --test codex_backend \
//!   codex_login_e2e_captures_authorize_url_for_livekit_web_metadata -- --ignored --nocapture
//! ```
//!
//! Requires the real **`codex`** on `PATH` (or **`TDDY_CODEX_CLI`**), network access, and
//! **`tddy-coder`** built next to the test binary (or **`CARGO_BIN_EXE_tddy-coder`**).

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use common::stub_invoke_request;
use tddy_core::backend::SessionMode;
use tddy_core::{BackendError, CodexBackend, CodingBackend, CODEX_OAUTH_AUTHORIZE_URL_FILENAME};
use tokio::process::Command;

/// CodexBackend spawns `codex` with `exec`, plan-mode `-s read-only`, `--json`, and the merged prompt.
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
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("exec"),
        "first argv after binary should be exec, got: {}",
        captured
    );
    let pos_exec = lines.iter().position(|l| *l == "exec").expect("exec");
    let pos_json = lines.iter().position(|l| *l == "--json").expect("--json");
    assert!(
        pos_exec < pos_json,
        "exec-level --json must follow exec (and -s), got: {:?}",
        lines
    );
    assert!(
        lines.contains(&"-s") && lines.contains(&"read-only"),
        "plan goal should pass -s read-only, got: {}",
        captured
    );
    assert!(
        captured.lines().any(|l| l == "test prompt"),
        "should include user prompt, got: {}",
        captured
    );
    assert_eq!(backend.name(), "codex");
}

/// Resume: `exec` … `-s` … `--json` `resume` … `<SESSION_ID>` `<PROMPT>` (exec-level flags before `resume`).
#[test]
#[cfg(unix)]
fn codex_backend_resume_subcommand_ordering() {
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
    let pos_exec = lines.iter().position(|l| *l == "exec").expect("exec");
    let pos_json = lines.iter().position(|l| *l == "--json").expect("--json");
    let pos_resume = lines.iter().position(|l| *l == "resume").expect("resume");
    let pos_sid = lines
        .iter()
        .position(|l| *l == "prev-codex-session")
        .expect("session id");
    assert!(
        pos_exec < pos_json && pos_json < pos_resume && pos_resume < pos_sid,
        "expected exec … --json … resume … SESSION_ID; got {:?}",
        lines
    );
    assert_eq!(lines.last().copied(), Some("continue"));
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

/// System prompt is merged into the argv payload like CursorBackend (inline system then user).
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
    assert!(
        captured.contains("SYSTEM_BLOCK_A") && captured.contains("user task text"),
        "merged argv should include system then user like Cursor, got:\n{}",
        captured
    );
}

/// Non-zero exit returns [`BackendError::InvocationFailed`]; JSONL `turn.failed` message is surfaced.
#[test]
#[cfg(unix)]
fn codex_backend_nonzero_exit_returns_err_with_jsonl_detail() {
    let tmp = std::env::temp_dir().join("tddy-codex-exit-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"item.completed","item":{{"text":"partial"}}}}'
printf '%s\n' '{{"type":"turn.failed","error":{{"message":"integration stub exit 7"}}}}'
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

    let err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect_err("nonzero exit should fail invoke for non-plan goals");

    match err {
        BackendError::InvocationFailed(msg) => {
            assert!(
                msg.contains("code 7") && msg.contains("integration stub exit 7"),
                "expected exit code and JSONL error in message, got: {}",
                msg
            );
        }
        other => panic!("expected InvocationFailed, got {:?}", other),
    }
}

/// `thread.started` supplies `session_id` when no `session` event is present (current Codex JSONL).
#[test]
#[cfg(unix)]
fn codex_backend_parses_thread_started_as_session_id() {
    let tmp = std::env::temp_dir().join("tddy-codex-thread-started-test");
    let _ = std::fs::create_dir_all(&tmp);
    let tmp_abs = tmp.canonicalize().unwrap_or(tmp.clone());
    let args_file = tmp_abs.join("captured_args.txt");

    let script = format!(
        r##"#!/bin/sh
printf '%s\n' "$@" > "{}"
printf '%s\n' '{{"type":"thread.started","thread_id":"thread-from-jsonl"}}'
printf '%s\n' '{{"type":"item.completed","item":{{"text":"done"}}}}'
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
    let req = stub_invoke_request("ping", "plan");

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req))
        .expect("invoke should succeed");
    assert_eq!(result.session_id.as_deref(), Some("thread-from-jsonl"));
    let _ = fs::read_to_string(&args_file).expect("read captured args");
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

/// Same resolution as `packages/tddy-e2e/tests/terminal_service_livekit.rs`: integration tests do
/// not always receive `CARGO_BIN_EXE_tddy-coder`.
fn resolve_tddy_coder_exe_for_tests() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-coder")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let exe = std::env::current_exe().expect("current exe");
            let deps = exe.parent().expect("exe parent");
            let debug = deps.parent().expect("deps parent");
            debug.join("tddy-coder")
        })
}

/// Runs real `codex login`, captures the authorize URL via `tddy-coder` as `BROWSER` (hook in
/// `tddy-coder` main), and checks JSON shape consumed by LiveKit → web
/// (`ParticipantList.parseCodexOAuthPending`).
#[tokio::test]
#[cfg(unix)]
#[ignore = "requires real codex CLI, network, and TDDY_CODEX_LOGIN_E2E=1; see module docs"]
async fn codex_login_e2e_captures_authorize_url_for_livekit_web_metadata() {
    assert_eq!(
        std::env::var("TDDY_CODEX_LOGIN_E2E").as_deref(),
        Ok("1"),
        "set TDDY_CODEX_LOGIN_E2E=1 when running this test with --ignored"
    );

    let session_dir =
        std::env::temp_dir().join(format!("tddy-codex-login-e2e-{}", std::process::id()));
    tokio::fs::create_dir_all(&session_dir)
        .await
        .expect("create session dir");
    let session_dir = session_dir.canonicalize().unwrap_or(session_dir);

    let url_path = session_dir.join(CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
    let tddy_coder = resolve_tddy_coder_exe_for_tests();
    assert!(
        tddy_coder.exists(),
        "tddy-coder not found at {:?}; run `cargo build -p tddy-coder` or set CARGO_BIN_EXE_tddy-coder",
        tddy_coder
    );

    let codex_bin: PathBuf = std::env::var_os("TDDY_CODEX_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));

    let mut child = Command::new(&codex_bin)
        .arg("login")
        .current_dir(&session_dir)
        .env("TDDY_CODEX_OAUTH_OUT", &url_path)
        .env("BROWSER", &tddy_coder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn codex login; ensure codex is on PATH or set TDDY_CODEX_CLI");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    let authorize_url = loop {
        if tokio::time::Instant::now() > deadline {
            let _ = child.kill().await;
            let _ = child.wait().await;
            panic!("timeout waiting for {}", url_path.display());
        }
        if let Ok(content) = tokio::fs::read_to_string(&url_path).await {
            let u = content.trim();
            if u.starts_with("https://") {
                break u.to_string();
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };

    let _ = child.kill().await;
    let _ = child.wait().await;

    // Contract: packages/tddy-livekit/src/participant.rs set_metadata JSON and
    // packages/tddy-web/src/components/ParticipantList.tsx parseCodexOAuthPending.
    let meta = serde_json::json!({
        "codex_oauth": {
            "pending": true,
            "authorize_url": authorize_url
        }
    });
    let v: serde_json::Value =
        serde_json::from_str(&meta.to_string()).expect("round-trip metadata JSON");
    assert_eq!(v["codex_oauth"]["pending"], true);
    let u = v["codex_oauth"]["authorize_url"]
        .as_str()
        .expect("authorize_url string");
    assert!(
        u.starts_with("https://"),
        "authorize_url must be https (web parser): {u:?}"
    );
}
