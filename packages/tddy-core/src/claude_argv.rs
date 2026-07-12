//! Shared builder for the base `claude` CLI argv (sandboxed runner + non-sandbox spawn path).

/// Build the base `claude` argv: `[binary, ("--model" model)?, <session flag> id, "--permission-mode" mode]`.
/// `--model` is omitted when `model` is empty. The session flag is `--resume <id>` when `resume`
/// is true (continue an existing on-disk transcript) and `--session-id <id>` otherwise (assign the
/// id to a fresh session); the two are mutually exclusive. Callers append any
/// `--append-system-prompt-file`, pass-through args, positional prompt, and MCP flags themselves.
pub fn build_claude_base_argv(
    binary_path: &str,
    model: &str,
    session_id: &str,
    permission_mode: &str,
    resume: bool,
) -> Vec<String> {
    let mut argv = vec![binary_path.to_string()];
    if !model.is_empty() {
        argv.push("--model".to_string());
        argv.push(model.to_string());
    }
    if resume {
        argv.push("--resume".to_string());
    } else {
        argv.push("--session-id".to_string());
    }
    argv.push(session_id.to_string());
    argv.push("--permission-mode".to_string());
    argv.push(permission_mode.to_string());
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token immediately following `flag` in an argv, or `None` if the flag is absent.
    fn value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1))
            .map(String::as_str)
    }

    fn contains_flag(argv: &[String], flag: &str) -> bool {
        argv.iter().any(|a| a == flag)
    }

    #[test]
    fn a_fresh_session_assigns_the_id_with_session_id() {
        // Given — a brand-new session (not a resume)
        let resume = false;

        // When
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "plan",
            resume,
        );

        // Then — the id is assigned via --session-id, and --resume is not used
        assert_eq!(
            value_after(&argv, "--session-id"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--resume"));
    }

    #[test]
    fn resuming_a_session_uses_resume_not_session_id() {
        // Given — a resume of an existing session whose transcript already exists on disk
        let resume = true;

        // When
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            resume,
        );

        // Then — the id is passed to --resume, and the conflicting --session-id flag is absent
        assert_eq!(
            value_after(&argv, "--resume"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--session-id"));
    }

    #[test]
    fn omits_the_model_flag_when_the_model_is_empty() {
        // Given — no model pinned
        let model = "";

        // When
        let argv = build_claude_base_argv(
            "claude",
            model,
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            false,
        );

        // Then — no --model flag is emitted
        assert!(!contains_flag(&argv, "--model"));
    }

    #[test]
    fn emits_the_model_flag_when_the_model_is_set() {
        // Given — a pinned model
        let model = "claude-opus-4-8";

        // When
        let argv = build_claude_base_argv(
            "claude",
            model,
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            false,
        );

        // Then — the model is passed via --model
        assert_eq!(value_after(&argv, "--model"), Some("claude-opus-4-8"));
    }

    #[test]
    fn preserves_the_permission_mode_for_a_fresh_session() {
        // Given — a fresh session pinned to a permission mode
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "plan",
            false,
        );

        // Then
        assert_eq!(value_after(&argv, "--permission-mode"), Some("plan"));
    }

    #[test]
    fn preserves_the_permission_mode_when_resuming() {
        // Given — a resume that must still pin the permission mode
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            true,
        );

        // Then
        assert_eq!(value_after(&argv, "--permission-mode"), Some("auto"));
    }
}
