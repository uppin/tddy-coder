//! Acceptance: a coding agent's spec declares the daemon variables its process is also started
//! with (`coding_agent.pass_env`), and can never declare a `TDDY_*` one (`#agent-tools 28/28`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ToolDef, ToolError};
use tddy_coding_agent_specs::{
    embedded_spec, load_agent_registry, CodingAgentSpec, SpecError, CODING_AGENT_EXTENSION,
};

/// A `<tddyhome>/tools` directory.
struct AToolsDir {
    dir: tempfile::TempDir,
}

fn a_tools_dir() -> AToolsDir {
    AToolsDir {
        dir: tempfile::tempdir().expect("a tools dir"),
    }
}

impl AToolsDir {
    fn with_agent(self, id: &str, pass_env: &str) -> Self {
        std::fs::write(
            self.dir.path().join(format!("{id}.yaml")),
            an_agent_file(id, pass_env),
        )
        .expect("a tool file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, file: &str) -> PathBuf {
        self.dir.path().join(file)
    }
}

/// An operator's agent `id` whose `coding_agent.pass_env` is the YAML flow list `pass_env`.
fn an_agent_file(id: &str, pass_env: &str) -> String {
    format!(
        r#"schema_version: 1
id: {id}
summary: a coding agent
binary:
  command: {id}
deps: []
run:
  transport: pty
  argv: ["{{binary}}"]
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
    pass_env: {pass_env}
"#
    )
}

fn an_agent_spec(id: &str, pass_env: &str) -> Result<CodingAgentSpec, SpecError> {
    let def: ToolDef =
        serde_yaml::from_str(&an_agent_file(id, pass_env)).expect("the tool file deserializes");
    CodingAgentSpec::of(&def)
}

/// The rejection `load_agent_registry` answers for `tools`, which must be a `coding_agent` one.
fn the_refusal_of(tools: &AToolsDir) -> (PathBuf, String) {
    let refused = load_agent_registry(tools.path()).unwrap_err();
    let ToolError::ExtensionRejected {
        file,
        owner,
        message,
    } = refused
    else {
        panic!("expected a coding_agent refusal, got {refused:?}");
    };
    assert_eq!(owner, CODING_AGENT_EXTENSION);
    (file, message)
}

fn a_daemon_env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

#[test]
fn a_pass_env_entry_matching_tddy_fails_spec_loading_naming_the_spec() {
    // Given — an operator agent that asks for the daemon's tool socket
    let tools = a_tools_dir().with_agent("lark", "[LARK_API_KEY, TDDY_SOCKET]");

    // When
    let (file, message) = the_refusal_of(&tools);

    // Then — the file is named, and so is the entry
    assert_eq!(file, tools.file("lark.yaml"));
    assert!(message.contains("TDDY_SOCKET"), "{message}");
}

#[test]
fn a_pass_env_prefix_pattern_covering_every_tddy_variable_fails_spec_loading() {
    for pattern in ["TDDY_*", "TDDY*", "T*", "*"] {
        // Given
        let tools = a_tools_dir().with_agent("lark", &format!("[\"{pattern}\"]"));

        // When
        let (file, message) = the_refusal_of(&tools);

        // Then
        assert_eq!(file, tools.file("lark.yaml"), "{pattern}");
        assert!(message.contains(pattern), "{pattern}: {message}");
    }
}

#[test]
fn a_pass_env_entry_that_is_neither_a_name_nor_a_prefix_pattern_fails_spec_loading() {
    for entry in ["\"LARK KEY\"", "\"*_KEY\"", "\"\""] {
        // Given
        let tools = a_tools_dir().with_agent("lark", &format!("[{entry}]"));

        // When
        let (file, _) = the_refusal_of(&tools);

        // Then
        assert_eq!(file, tools.file("lark.yaml"), "{entry}");
    }
}

#[test]
fn a_spec_refusing_a_tddy_pass_env_entry_names_its_tool() {
    // When
    let refused = an_agent_spec("lark", "[TDDY_SESSION_DIR]").unwrap_err();

    // Then
    let SpecError::Invalid { tool, message } = refused else {
        panic!("expected an invalid spec, got {refused:?}");
    };
    assert_eq!(tool, "lark");
    assert!(message.contains("TDDY_SESSION_DIR"), "{message}");
}

#[test]
fn a_spec_declaring_pass_env_loads_with_its_entries() {
    // When
    let spec = an_agent_spec("lark", "[LARK_API_KEY, \"LARK_*\"]").expect("the spec is valid");

    // Then — guard: names and prefix patterns are accepted, in the order declared
    assert_eq!(
        spec.pass_env(),
        &["LARK_API_KEY".to_string(), "LARK_*".to_string()]
    );
}

#[test]
fn passed_env_is_the_daemon_variables_pass_env_names_by_name_and_by_prefix() {
    // Given
    let spec = an_agent_spec("lark", "[LARK_API_KEY, \"LARK_HOME_*\"]").expect("valid");
    let daemon = a_daemon_env(&[
        ("PATH", "/usr/bin"),
        ("LARK_API_KEY", "sk-lark"),
        ("LARK_HOME_DIR", "/var/lark"),
        ("LARK_OTHER", "not-named"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
    ]);

    // When
    let passed = spec.passed_env(daemon);

    // Then
    assert_eq!(
        passed,
        vec![
            ("LARK_API_KEY".to_string(), "sk-lark".to_string()),
            ("LARK_HOME_DIR".to_string(), "/var/lark".to_string()),
        ]
    );
}

#[test]
fn passed_env_is_empty_for_a_spec_declaring_none() {
    // Given
    let spec = an_agent_spec("lark", "[]").expect("valid");

    // When
    let passed = spec.passed_env(a_daemon_env(&[("LARK_API_KEY", "sk-lark")]));

    // Then — guard
    assert_eq!(passed, Vec::<(String, String)>::new());
}

#[test]
fn every_builtin_spec_declares_its_provider_key_variables() {
    // The provider keys each built-in agent reads, from its vendor's documentation. An agent
    // that is missing from this table declares no provider key (its login is a file under HOME).
    let expected: &[(&str, &[&str])] = &[
        ("claude", &["ANTHROPIC_API_KEY"]),
        ("claude-acp", &["ANTHROPIC_API_KEY"]),
        ("codex", &["OPENAI_API_KEY"]),
        ("codex-acp", &["OPENAI_API_KEY"]),
        ("cursor", &["CURSOR_API_KEY"]),
        ("opencode", &["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]),
    ];
    for (id, keys) in expected {
        // When
        let spec = embedded_spec(id).expect("the spec is built in");

        // Then
        for key in *keys {
            assert!(
                spec.pass_env().iter().any(|entry| entry == key),
                "{id} declares {key} in pass_env: {:?}",
                spec.pass_env()
            );
        }
    }
}

#[test]
fn no_builtin_spec_passes_a_tddy_variable() {
    // Then — guard: every built-in loads, so none declares one
    for id in [
        "claude",
        "claude-acp",
        "codex",
        "codex-acp",
        "cursor",
        "muse",
        "opencode",
        "oz",
        "warp",
    ] {
        let spec = embedded_spec(id).expect("the spec is built in");
        assert!(
            spec.pass_env()
                .iter()
                .all(|entry| !entry.starts_with("TDDY")),
            "{id}: {:?}",
            spec.pass_env()
        );
    }
}
