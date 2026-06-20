//! Acceptance tests: RemoteContextDir and run_remote bootstrap (Phase 5).
//!
//! AC24: read-only context dir created by RemoteContextDir has REMOTE_APPENDIX appended to
//! CLAUDE.md/AGENTS.md; files are chmod 0o444; Drop cleans up the directory.
//!
//! AC25: run_remote builds the allowlist from `tddy-tools remote list-tools` output, prepending
//! `mcp__tddy-tools__` to each discovered tool name.

use std::fs;
use tddy_coder::remote::{RemoteContextDir, REMOTE_APPENDIX};

/// AC24: `RemoteContextDir::create` produces a temporary directory containing CLAUDE.md
/// with `REMOTE_APPENDIX` appended to the end.
#[test]
fn remote_context_dir_appends_remote_appendix_to_claude_md() {
    // Given
    let source_dir = tempfile::tempdir().unwrap();
    let original_content = "# CLAUDE.md\n\nThis is the original instructions.\n";
    fs::write(source_dir.path().join("CLAUDE.md"), original_content)
        .expect("failed to write CLAUDE.md");

    // When
    let ctx =
        RemoteContextDir::create(source_dir.path()).expect("RemoteContextDir::create must succeed");
    let claude_md = fs::read_to_string(ctx.path().join("CLAUDE.md"))
        .expect("CLAUDE.md must exist in remote context dir");

    // Then
    assert!(
        claude_md.contains("This is the original instructions."),
        "CLAUDE.md must preserve original content; got:\n{}",
        claude_md
    );
    assert!(
        claude_md.contains("mcp__tddy-tools__"),
        "CLAUDE.md must contain REMOTE_APPENDIX (with mcp__tddy-tools__ reference); got:\n{}",
        claude_md
    );
    assert!(
        claude_md.ends_with(REMOTE_APPENDIX) || claude_md.contains(REMOTE_APPENDIX),
        "REMOTE_APPENDIX must appear in CLAUDE.md; got:\n{}",
        claude_md
    );
}

/// AC24: Files in the remote context dir must be read-only (mode 0o444 on Unix).
#[cfg(unix)]
#[test]
fn remote_context_dir_files_are_read_only() {
    use std::os::unix::fs::PermissionsExt;

    // Given
    let source_dir = tempfile::tempdir().unwrap();
    fs::write(source_dir.path().join("CLAUDE.md"), "# Test\n").expect("write source");

    // When
    let ctx =
        RemoteContextDir::create(source_dir.path()).expect("RemoteContextDir::create must succeed");
    let claude_md_path = ctx.path().join("CLAUDE.md");
    let metadata = fs::metadata(&claude_md_path).expect("CLAUDE.md must exist");

    // Then
    let mode = metadata.permissions().mode();
    let writable_bits = mode & 0o222;
    assert_eq!(
        writable_bits, 0,
        "CLAUDE.md must be read-only (no write bits set); mode = {:o}",
        mode
    );
}

/// AC24: `RemoteContextDir` cleans up the temporary directory on Drop.
#[test]
fn remote_context_dir_cleaned_up_on_drop() {
    // Given
    let source_dir = tempfile::tempdir().unwrap();
    fs::write(source_dir.path().join("CLAUDE.md"), "# Test\n").expect("write source");

    // When
    let path = {
        let ctx = RemoteContextDir::create(source_dir.path())
            .expect("RemoteContextDir::create must succeed");
        ctx.path().to_path_buf()
    }; // ctx dropped here

    // Then
    assert!(
        !path.exists(),
        "remote context dir must be cleaned up on Drop; path still exists: {}",
        path.display()
    );
}

/// AC25: `build_remote_allowlist` prefixes each discovered tool name with `mcp__tddy-tools__`
/// and always includes `AskUserQuestion`.
#[test]
fn build_remote_allowlist_prefixes_discovered_tools() {
    use tddy_coder::remote::build_remote_allowlist;

    // Given
    let discovered_tools = ["Read", "Write", "Grep", "Shell"];

    // When
    let allowlist = build_remote_allowlist(&discovered_tools);

    // Then — each discovered tool must be prefixed
    for tool in &discovered_tools {
        let prefixed = format!("mcp__tddy-tools__{}", tool);
        assert!(
            allowlist.contains(&prefixed),
            "allowlist must contain '{}'; got: {:?}",
            prefixed,
            allowlist
        );
    }
    assert!(
        allowlist.contains(&"AskUserQuestion".to_string()),
        "allowlist must always include AskUserQuestion; got: {:?}",
        allowlist
    );
    for tool in &discovered_tools {
        assert!(
            !allowlist.contains(&tool.to_string()),
            "allowlist must not contain bare (unprefixed) tool name '{}'; got: {:?}",
            tool,
            allowlist
        );
    }
}

/// AC25: `RemoteConfig` holds the CLI flags for remote mode.
#[test]
fn remote_config_holds_all_remote_flags() {
    use tddy_coder::config::RemoteConfig;

    // Given
    let cfg = RemoteConfig {
        daemon_url: Some("http://relay.local:9000".to_string()),
        session_id: Some("sess-existing-123".to_string()),
        session_token: Some("tok-abc".to_string()),
        daemon_instance_id: Some("remote-daemon-id-xyz".to_string()),
    };

    // Then
    assert_eq!(cfg.daemon_url.as_deref(), Some("http://relay.local:9000"));
    assert_eq!(cfg.session_id.as_deref(), Some("sess-existing-123"));
    assert_eq!(cfg.session_token.as_deref(), Some("tok-abc"));
    assert_eq!(
        cfg.daemon_instance_id.as_deref(),
        Some("remote-daemon-id-xyz")
    );
}
