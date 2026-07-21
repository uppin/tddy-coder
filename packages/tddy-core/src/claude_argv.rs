//! Shared builder for the base `claude` CLI argv (sandboxed runner + non-sandbox spawn path).

/// Build the base `claude` argv: `[binary, ("--model" model)?, <session flag> id, <permission flag>]`.
/// `--model` is omitted when `model` is empty. The session flag is `--resume <id>` when `resume`
/// is true (continue an existing on-disk transcript) and `--session-id <id>` otherwise (assign the
/// id to a fresh session); the two are mutually exclusive.
///
/// The permission flag is `--dangerously-skip-permissions` when `dangerously_skip_permissions` is
/// true — bypassing all prompts — and `--permission-mode <mode>` otherwise. The two are mutually
/// exclusive: the claude CLI rejects being given both, so `permission_mode` is ignored when the skip
/// flag is set.
///
/// Callers append any `--append-system-prompt-file`, pass-through args, positional prompt, and MCP
/// flags themselves.
pub fn build_claude_base_argv(
    binary_path: &str,
    model: &str,
    session_id: &str,
    permission_mode: &str,
    dangerously_skip_permissions: bool,
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
    if dangerously_skip_permissions {
        argv.push("--dangerously-skip-permissions".to_string());
    } else {
        argv.push("--permission-mode".to_string());
        argv.push(permission_mode.to_string());
    }
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
            false,
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
            false,
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
            false,
            true,
        );

        // Then
        assert_eq!(value_after(&argv, "--permission-mode"), Some("auto"));
    }

    #[test]
    fn dangerously_skip_permissions_emits_the_skip_flag() {
        // Given — a session that opts into skipping all permission prompts
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            true,
            false,
        );

        // Then — the skip flag is present
        assert!(contains_flag(&argv, "--dangerously-skip-permissions"));
    }

    #[test]
    fn dangerously_skip_permissions_omits_the_conflicting_permission_mode_flag() {
        // Given — the skip flag is set alongside a non-empty permission mode
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "plan",
            true,
            false,
        );

        // Then — --permission-mode is dropped (the claude CLI rejects both together)
        assert!(!contains_flag(&argv, "--permission-mode"));
    }

    #[test]
    fn without_the_skip_flag_no_skip_flag_is_emitted() {
        // Given — the ordinary permission-mode path
        let argv = build_claude_base_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            "auto",
            false,
            false,
        );

        // Then — the skip flag is absent and --permission-mode is used
        assert!(!contains_flag(&argv, "--dangerously-skip-permissions"));
        assert_eq!(value_after(&argv, "--permission-mode"), Some("auto"));
    }
}
