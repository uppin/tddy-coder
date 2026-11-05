//! Acceptance tests for the `codex` spec turn (`codex exec --json`, `codex exec resume <id>`).
//!
//! ## Stub binary (not the real OpenAI Codex CLI)
//!
//! These tests run a **small shell script** named `codex` that records argv and prints fixture
//! JSONL. That matches [`cursor_backend.rs`] and is the supported way to test here:
//!
//! - **Deterministic** — no `codex login`, API keys, or network calls in CI.
//! - **Fast** — no heavyweight `codex` / WebRTC / LiveKit startup.
//! - **Encapsulation** — exercises the real `SpecHeadlessBackend` code path (the `codex` spec) (`Command`, pipes,
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
//! cargo build -p tddy-coder-cli
//! TDDY_CODEX_LOGIN_E2E=1 cargo test -p tddy-integration-tests --test codex_backend \
//!   codex_login_e2e_captures_authorize_url_for_livekit_web_metadata -- --ignored --nocapture
//! ```
//!
//! Requires the real **`codex`** on `PATH` (or **`TDDY_CODEX_CLI`**), network access, and
//! **`tddy-coder`** built next to the test binary (or **`CARGO_BIN_EXE_tddy-coder`**).

mod common;

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use common::{
    a_spec_backend, in_spec_workspace, spec_argv, spec_workspace_root, stub_invoke_request,
};
use tddy_core::backend::SessionMode;
use tddy_core::{
    BackendError, CodingBackend, InvokeRequest, InvokeResponse, CODEX_OAUTH_AUTHORIZE_URL_FILENAME,
};
use tokio::process::Command;

/// Runs `req` over a stub `codex` that prints `stub_script` after recording its argv.
fn run_codex(
    stub_script: &str,
    req: InvokeRequest,
) -> (Result<InvokeResponse, BackendError>, tempfile::TempDir) {
    let (backend, dir) = a_spec_backend("codex", stub_script);
    let req = in_spec_workspace(&dir, req);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req));
    (result, dir)
}

/// The codex spec turn runs `codex exec` with plan-mode `-s read-only`, `--json`, and the merged prompt.
#[test]
#[cfg(unix)]
fn codex_backend_spawns_exec_with_json_and_prompt() {
    // Given
    let script = r##"printf '%s\n' '{"type":"session","session_id":"codex-sess-1"}'
printf '%s\n' '{"type":"item.completed","item":{"text":"parsed output"}}'
exit 0"##;
    let (backend, _d) = a_spec_backend("codex", "exit 0");

    // When
    let (result, dir) = run_codex(script, stub_invoke_request("test prompt", "plan"));
    let result = result.expect("invoke should succeed");

    // Then
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "parsed output");
    assert_eq!(result.session_id.as_deref(), Some("codex-sess-1"));

    let lines = spec_argv(&dir);
    assert_eq!(
        lines.first().map(String::as_str),
        Some("exec"),
        "first argv after binary should be exec, got: {lines:?}"
    );
    let pos_exec = lines.iter().position(|l| l == "exec").expect("exec");
    let pos_json = lines.iter().position(|l| l == "--json").expect("--json");
    assert!(
        pos_exec < pos_json,
        "exec-level --json must follow exec (and -s), got: {lines:?}"
    );
    let sandbox = lines.iter().position(|l| l == "-s").expect("-s");
    assert_eq!(
        lines.get(sandbox + 1).map(String::as_str),
        Some("read-only"),
        "plan goal should pass -s read-only, got: {lines:?}"
    );
    let workdir = lines.iter().position(|l| l == "-C").expect("-C");
    assert_eq!(
        lines.get(workdir + 1).map(PathBuf::from),
        Some(spec_workspace_root(&dir).join("repo")),
        "codex runs in the request's working directory"
    );
    assert!(
        lines.iter().any(|l| l == "test prompt"),
        "should include user prompt, got: {lines:?}"
    );
    assert_eq!(backend.name(), "codex");
}

/// A goal that edits runs with `-s workspace-write`.
#[test]
#[cfg(unix)]
fn codex_backend_uses_workspace_write_for_a_goal_that_edits() {
    // Given
    let script = r##"printf '%s\n' '{"type":"item.completed","item":{"text":"ok"}}'"##;

    // When
    let (result, dir) = run_codex(script, stub_invoke_request("go", "green"));
    result.expect("invoke should succeed");

    // Then
    let lines = spec_argv(&dir);
    let sandbox = lines.iter().position(|l| l == "-s").expect("-s");
    assert_eq!(
        lines.get(sandbox + 1).map(String::as_str),
        Some("workspace-write")
    );
}

/// Resume: `exec` … `-s` … `--json` `resume` … `<SESSION_ID>` `<PROMPT>` (exec-level flags before `resume`).
#[test]
#[cfg(unix)]
fn codex_backend_resume_subcommand_ordering() {
    // Given
    let script = r##"printf '%s\n' '{"type":"item.completed","item":{"text":"ok"}}'"##;
    let mut req = stub_invoke_request("continue", "plan");
    req.session = Some(SessionMode::Resume("prev-codex-session".to_string()));

    // When
    let (result, dir) = run_codex(script, req);
    result.expect("invoke should succeed");

    // Then
    let lines = spec_argv(&dir);
    let pos = |what: &str| {
        lines
            .iter()
            .position(|l| l == what)
            .unwrap_or_else(|| panic!("{what} in {lines:?}"))
    };
    let (pos_exec, pos_json, pos_resume, pos_sid) = (
        pos("exec"),
        pos("--json"),
        pos("resume"),
        pos("prev-codex-session"),
    );
    assert!(
        pos_exec < pos_json && pos_json < pos_resume && pos_resume < pos_sid,
        "expected exec … --json … resume … SESSION_ID; got {lines:?}"
    );
    assert_eq!(lines.last().map(String::as_str), Some("continue"));
}

#[test]
#[cfg(unix)]
fn codex_backend_includes_model_flag_when_set() {
    // Given
    let script = r##"printf '%s\n' '{"type":"item.completed","item":{"text":"x"}}'"##;
    let mut req = stub_invoke_request("hi", "red");
    req.model = Some("gpt-5".to_string());

    // When
    let (result, dir) = run_codex(script, req);
    result.expect("invoke should succeed");

    // Then
    let lines = spec_argv(&dir);
    let mpos = lines.iter().position(|l| l == "-m").expect("-m flag");
    assert_eq!(lines.get(mpos + 1).map(String::as_str), Some("gpt-5"));
}

/// System prompt is merged into the argv payload like Cursor (inline system then user).
#[test]
#[cfg(unix)]
fn codex_backend_merges_system_prompt_like_cursor() {
    // Given
    let script = r##"printf '%s\n' '{"type":"item.completed","item":{"text":"ok"}}'"##;
    let mut req = stub_invoke_request("user task text", "plan");
    req.system_prompt = Some("SYSTEM_BLOCK_A".to_string());

    // When
    let (result, dir) = run_codex(script, req);
    result.expect("invoke should succeed");

    // Then
    let lines = spec_argv(&dir);
    assert_eq!(
        lines[lines.len() - 3..],
        [
            "SYSTEM_BLOCK_A".to_string(),
            String::new(),
            "user task text".to_string()
        ],
        "merged prompt is system, blank line, user, got: {lines:?}"
    );
}

/// Non-zero exit returns [`BackendError::InvocationFailed`]; JSONL `turn.failed` message is surfaced.
#[test]
#[cfg(unix)]
fn codex_backend_nonzero_exit_returns_err_with_jsonl_detail() {
    // Given
    // (no completed item: a structured response with a non-zero exit is accepted, see below)
    let script = r##"printf '%s\n' '{"type":"turn.failed","error":{"message":"integration stub exit 7"}}'
exit 7"##;

    // When
    let (result, _dir) = run_codex(script, stub_invoke_request("x", "green"));
    let err = result.expect_err("nonzero exit should fail invoke for non-plan goals");

    // Then
    match err {
        BackendError::InvocationFailed(msg) => {
            assert!(
                msg.starts_with("codex exited with code 7: ")
                    && msg.contains("integration stub exit 7"),
                "expected exit code and JSONL error in message, got: {msg}"
            );
        }
        other => panic!("expected InvocationFailed, got {other:?}"),
    }
}

/// The codex spec accepts a non-zero exit that still printed a structured response
/// (`exit_ok_if_structured` is the spec's property now, not the goal hint's).
#[test]
#[cfg(unix)]
fn codex_backend_keeps_a_structured_response_when_the_exit_is_nonzero() {
    // Given
    let script = r##"printf '%s\n' '{"type":"item.completed","item":{"text":"partial"}}'
exit 7"##;

    // When
    let (result, _dir) = run_codex(script, stub_invoke_request("x", "green"));

    // Then
    let response = result.expect("a structured response is accepted");
    assert_eq!(
        (response.output.as_str(), response.exit_code),
        ("partial", 7)
    );
}

/// `thread.started` supplies `session_id` when no `session` event is present (current Codex JSONL).
#[test]
#[cfg(unix)]
fn codex_backend_parses_thread_started_as_session_id() {
    // Given
    let script = r##"printf '%s\n' '{"type":"thread.started","thread_id":"thread-from-jsonl"}'
printf '%s\n' '{"type":"item.completed","item":{"text":"done"}}'
exit 0"##;

    // When
    let (result, _dir) = run_codex(script, stub_invoke_request("ping", "plan"));

    // Then
    assert_eq!(
        result.expect("invoke should succeed").session_id.as_deref(),
        Some("thread-from-jsonl")
    );
}

/// A missing `codex` binary is now found out when the backend is BUILT: the spec's binary must
/// resolve on the host, so no turn is ever started against a binary that is not there.
#[test]
#[cfg(unix)]
fn codex_backend_reports_binary_not_found_when_built() {
    // When
    let err = common::a_spec_backend_over_a_missing_binary("codex")
        .map(|_| ())
        .expect_err("expected the build to fail for a missing codex binary");

    // Then
    assert!(
        err.contains("codex") && err.contains("definitely-no-such-binary-9f3a2c1b"),
        "the message names the agent and the binary its spec could not resolve: {err}"
    );
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
    // Given

    // Then
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

    // When
    let tddy_coder = resolve_tddy_coder_exe_for_tests();
    assert!(
        tddy_coder.exists(),
        "tddy-coder not found at {:?}; run `cargo build -p tddy-coder-cli` or set CARGO_BIN_EXE_tddy-coder",
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
