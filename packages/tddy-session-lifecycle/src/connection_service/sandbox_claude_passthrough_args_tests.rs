use super::sandbox_claude_passthrough_args;

#[test]
fn forwards_client_claude_args_verbatim() {
    // Given — a client that passed extra claude flags and no prompt
    let claude_args = vec!["--add-dir".to_string(), "/repo/extra".to_string()];

    // When
    let tokens = sandbox_claude_passthrough_args(&claude_args, "");

    // Then
    assert_eq!(tokens, vec!["--add-dir", "/repo/extra"]);
}

#[test]
fn appends_the_initial_prompt_last_as_a_trailing_positional() {
    // Given — both extra flags and an initial prompt
    let claude_args = vec!["--add-dir".to_string(), "/repo/extra".to_string()];

    // When
    let tokens = sandbox_claude_passthrough_args(&claude_args, "build feature X");

    // Then — the prompt must land after every flag so it stays a positional
    assert_eq!(tokens, vec!["--add-dir", "/repo/extra", "build feature X"]);
}

#[test]
fn omits_a_blank_initial_prompt() {
    // Given — no client args and a whitespace-only prompt
    // When
    let tokens = sandbox_claude_passthrough_args(&[], "   ");

    // Then
    assert!(tokens.is_empty());
}
