//! The OpenAI Codex CLI's sign-in: `codex login` with browser OAuth, and the files the Codex flow
//! leaves in a session directory.
//!
//! A headless `codex exec --json` turn is run by [`crate::SpecHeadlessBackend`] from `codex.yaml`;
//! nothing here runs a turn.
//!
//! **Auth:** `codex exec` does **not** emit an OpenAI browser OAuth URL when credentials are
//! missing — you get JSONL / stderr API errors (e.g. 401) only. The sign-in link is produced by
//! **`codex login`** (stdout + `BROWSER`); see [`spawn_codex_oauth_login`] and the `tddy-coder`
//! `BROWSER` hook for capturing that URL into the session dir.

use super::InvokeRequest;
use crate::error::BackendError;
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) mod login_text;
pub use login_text::{scrape_codex_oauth_authorize_url_from_text, write_codex_thread_id_file};

/// The executable name `codex` is looked up as on `PATH` when nothing names it.
pub const CODEX_DEFAULT_CLI_BINARY: &str = "codex";

/// Basename under the TDDY artifact session directory; stores Codex CLI `thread_id` for `codex exec resume`.
pub const CODEX_THREAD_ID_FILENAME: &str = "codex_thread_id";

/// Basename written when Codex invokes `BROWSER` with the OpenAI authorize URL (`tddy-coder` hook).
pub const CODEX_OAUTH_AUTHORIZE_URL_FILENAME: &str = "codex_oauth_authorize.url";

/// Run `codex login` with **browser OAuth** (the default `codex login` flow, **not** `--device-auth`).
///
/// On Unix, when [`std::env::current_exe`] succeeds, sets `BROWSER` to the current executable and
/// `TDDY_CODEX_OAUTH_OUT` to `{session_dir}/{[`CODEX_OAUTH_AUTHORIZE_URL_FILENAME`]}`, matching
/// a headless codex turn so the `tddy-coder` pre-main hook records the authorize URL.
///
/// Stdout is scanned in a background thread for an `https://` line as a fallback if the CLI only
/// prints the link.
///
/// You must [`std::process::Child::wait`] on the returned child: Codex keeps a localhost callback
/// server until the user finishes signing in at `http://localhost:<port>/…`.
pub fn spawn_codex_oauth_login(
    binary: &Path,
    session_dir: &Path,
) -> Result<std::process::Child, BackendError> {
    std::fs::create_dir_all(session_dir).map_err(|e| {
        BackendError::InvocationFailed(format!(
            "codex login: could not create session_dir {}: {}",
            session_dir.display(),
            e
        ))
    })?;
    let oauth_out = session_dir.join(CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
    let mut cmd = Command::new(binary);
    cmd.arg("login");
    cmd.env("PATH", super::path_with_exe_dir());
    #[cfg(unix)]
    if let Ok(exe) = std::env::current_exe() {
        log::info!(
            target: "tddy_core::backend::codex",
            "[tddy-codex] codex login OAuth capture: BROWSER={}, TDDY_CODEX_OAUTH_OUT={}",
            exe.display(),
            oauth_out.display()
        );
        cmd.env("TDDY_CODEX_OAUTH_OUT", oauth_out.as_os_str());
        cmd.env("BROWSER", exe.as_os_str());
    }
    cmd.env("TDDY_SESSION_DIR", session_dir.as_os_str());
    cmd.current_dir(session_dir);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BackendError::BinaryNotFound(binary.to_string_lossy().to_string())
        } else {
            BackendError::InvocationFailed(format!("codex login spawn: {}", e))
        }
    })?;

    let out = child
        .stdout
        .take()
        .ok_or_else(|| BackendError::InvocationFailed("codex login: no stdout".into()))?;
    let err = child.stderr.take();
    let oauth_clone = oauth_out.clone();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(out);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(url) = scrape_codex_oauth_authorize_url_from_text(&line) {
                if let Err(e) = std::fs::write(&oauth_clone, &url) {
                    log::warn!(
                        "[tddy-codex] codex login: could not write authorize URL to {}: {}",
                        oauth_clone.display(),
                        e
                    );
                } else {
                    log::debug!("[tddy-codex] codex login: captured URL from stdout");
                }
            }
        }
    });
    std::thread::spawn(move || {
        if let Some(h) = err {
            let reader = std::io::BufReader::new(h);
            for line in reader.lines().map_while(Result::ok) {
                log::debug!("[tddy-codex] codex login stderr: {}", line);
            }
        }
    });

    Ok(child)
}

/// Merge system prompt / path with user prompt (the system text in front, a blank line between) for the ACP codex backends.
pub(crate) fn merge_codex_prompt(request: &InvokeRequest) -> Result<String, BackendError> {
    let system_content: Option<String> = if let Some(ref path) = request.system_prompt_path {
        Some(std::fs::read_to_string(path).map_err(|e| {
            BackendError::InvocationFailed(format!(
                "failed to read system_prompt_path {}: {}",
                path.display(),
                e
            ))
        })?)
    } else {
        request.system_prompt.clone()
    };

    let merged = match system_content {
        Some(ref sys) => format!("{}\n\n{}", sys, request.prompt),
        None => request.prompt.clone(),
    };
    log::debug!(
        "[tddy-codex] merged prompt length: {} (user prompt {} bytes)",
        merged.len(),
        request.prompt.len()
    );
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::InvokeRequest;
    use tddy_workflow::{GoalHints, GoalId, PermissionHint};

    #[test]
    fn codex_merge_prompt_puts_the_system_text_in_front_of_the_user_line() {
        // Given
        let request = InvokeRequest {
            prompt: "user line".to_string(),
            system_prompt: Some("system instruction".to_string()),
            system_prompt_path: None,
            goal_id: GoalId::new("plan"),
            submit_key: GoalId::new("plan"),
            hints: GoalHints {
                display_name: "Plan".to_string(),
                permission: PermissionHint::ReadOnly,
                allowed_tools: vec![],
                default_model: None,
                agent_output: false,
                agent_cli_plan_mode: true,
                claude_nonzero_exit_ok_if_structured_response: true,
            },
            model: None,
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            session_dir: None,
            remote: None,
        };

        // When
        let merged = merge_codex_prompt(&request).expect("merge");

        // Then
        assert_eq!(merged, "system instruction\n\nuser line");
    }

    #[test]
    fn scrape_codex_oauth_url_from_login_line() {
        // Given
        let line = "https://auth.openai.com/oauth/authorize?x=1&y=2";

        // When
        let url = scrape_codex_oauth_authorize_url_from_text(line);

        // Then
        assert_eq!(url.as_deref(), Some(line));
    }

    #[test]
    fn scrape_codex_oauth_url_strips_trailing_punct() {
        // Given
        let line = r#"See: https://auth.openai.com/x?y=1)"#;

        // When
        let url = scrape_codex_oauth_authorize_url_from_text(line);

        // Then
        assert_eq!(url.as_deref(), Some("https://auth.openai.com/x?y=1"));
    }

    #[test]
    fn codex_openai_auth_remediation_matches_401_missing_bearer() {
        // Given
        let detail = "unexpected status 401 Unauthorized: Missing bearer or basic authentication";

        // When
        let hint = login_text::codex_openai_auth_remediation(detail);

        // Then
        assert!(
            hint.is_some(),
            "should produce a remediation hint for 401 Unauthorized"
        );
    }

    #[test]
    fn codex_openai_auth_remediation_ignores_unrelated_401() {
        // When
        let hint = login_text::codex_openai_auth_remediation("HTTP 401");

        // Then
        assert!(
            hint.is_none(),
            "bare '401' without auth keywords should produce no hint"
        );
    }
}
