//! The spec surface the generic CLI launch reads for an agent with Claude Code's or Cursor's needs
//! (`#agent-tools 14/19`): the `managed_workflow` capability and `coding_agent.conversation`.
//!
//! Feature doc: `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (conversation slot,
//! `managed_workflow`, at wrap).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_actions::tool::RunSection;
use tddy_coding_agent_specs::slots::conversation::{
    ConversationSlot, RotationHook, RotationSlot, DEFAULT_CONVERSATION_MINT_DEADLINE_SECS,
};
use tddy_coding_agent_specs::{
    builtin_specs, Capability, CodingAgentRegistry, CodingAgentSection, CodingAgentSpec,
};

/// The built-in `id`'s embedded YAML.
fn the_builtin_yaml(id: &str) -> &'static str {
    let file_name = format!("/{id}.yaml");
    builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with(&file_name))
        .map(|(_, yaml)| yaml)
        .unwrap_or_else(|| panic!("{id}.yaml is built in"))
}

/// The coding-agent view of `yaml`, read straight from its `run` and `coding_agent` sections.
fn the_spec_in(id: &str, yaml: &str) -> CodingAgentSpec {
    let file: serde_yaml::Value = serde_yaml::from_str(yaml).expect("the spec is YAML");
    let run: RunSection = serde_yaml::from_value(file["run"].clone()).expect("its run section");
    let agent: CodingAgentSection =
        serde_yaml::from_value(file["extensions"]["coding_agent"].clone())
            .expect("its coding_agent section");
    CodingAgentSpec {
        id: id.to_string(),
        run,
        agent,
    }
}

/// `opencode.yaml` with `line` added to its `coding_agent` section (indented four spaces).
fn opencode_yaml_declaring(line: &str) -> String {
    let yaml = the_builtin_yaml("opencode");
    let section = "  coding_agent:\n";
    assert!(
        yaml.contains(section),
        "opencode.yaml has a coding_agent section"
    );
    yaml.replacen(section, &format!("{section}{line}"), 1)
}

/// A `<tddyhome>/tools` holding `opencode.yaml` as `yaml`, loaded.
fn loading_opencode_as(yaml: &str) -> Result<usize, String> {
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("opencode.yaml"), yaml).unwrap();
    CodingAgentRegistry::load(Path::new(&tools))
        .map(|registry| registry.all().len())
        .map_err(|error| error.to_string())
}

#[test]
fn managed_workflow_is_written_as_its_snake_case_word() {
    // When
    let word = (
        Capability::ManagedWorkflow.as_str(),
        serde_yaml::from_str::<Capability>("managed_workflow").ok(),
        Capability::ALL.contains(&Capability::ManagedWorkflow),
    );

    // Then
    assert_eq!(
        word,
        ("managed_workflow", Some(Capability::ManagedWorkflow), true)
    );
}

#[test]
fn claude_and_cursor_declare_managed_workflow_and_no_other_builtin_does() {
    // When
    let declaring: Vec<String> = [
        "claude",
        "claude-acp",
        "codex",
        "codex-acp",
        "cursor",
        "muse",
        "opencode",
        "oz",
        "warp",
    ]
    .into_iter()
    .filter(|id| the_spec_in(id, the_builtin_yaml(id)).has(Capability::ManagedWorkflow))
    .map(str::to_string)
    .collect();

    // Then
    assert_eq!(declaring, vec!["claude".to_string(), "cursor".to_string()]);
}

#[test]
fn each_builtin_cli_agent_converses_the_way_its_launch_resumes_it() {
    // When
    let conversations: BTreeMap<&str, ConversationSlot> =
        ["claude", "cursor", "muse", "opencode", "warp"]
            .into_iter()
            .map(|id| (id, the_spec_in(id, the_builtin_yaml(id)).conversation()))
            .collect();

    // Then — claude by the daemon's session id, followed through `/clear` (`#orchestrators` 6/11),
    // cursor by a minted chat, opencode and muse by their worktree, warp by the token its resume
    // adapter reads
    assert_eq!(
        conversations,
        BTreeMap::from([
            (
                "claude",
                ConversationSlot::DaemonSessionId {
                    rotation: Some(RotationSlot {
                        ends: RotationHook {
                            hook: "SessionEnd".to_string(),
                            when: BTreeMap::from([("reason".to_string(), "clear".to_string())]),
                            id_field: "session_id".to_string(),
                        },
                        begins: RotationHook {
                            hook: "SessionStart".to_string(),
                            when: BTreeMap::from([("source".to_string(), "clear".to_string())]),
                            id_field: "session_id".to_string(),
                        },
                    }),
                }
            ),
            (
                "cursor",
                ConversationSlot::Minted {
                    adapter: "cursor-create-chat".to_string(),
                    mint_deadline_secs: None,
                }
            ),
            ("muse", ConversationSlot::Worktree),
            ("opencode", ConversationSlot::Worktree),
            ("warp", ConversationSlot::ResumeToken),
        ])
    );
}

#[test]
fn a_minted_conversation_is_bounded_by_its_declared_deadline_or_the_default() {
    // Given
    let declared = ConversationSlot::Minted {
        adapter: "cursor-create-chat".to_string(),
        mint_deadline_secs: Some(5),
    };
    let undeclared = ConversationSlot::Minted {
        adapter: "cursor-create-chat".to_string(),
        mint_deadline_secs: None,
    };

    // When
    let deadlines = (
        declared.mint_deadline(),
        undeclared.mint_deadline(),
        ConversationSlot::DaemonSessionId { rotation: None }.mint_deadline(),
    );

    // Then
    assert_eq!(
        deadlines,
        (
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(DEFAULT_CONVERSATION_MINT_DEADLINE_SECS)),
            None,
        )
    );
}

#[test]
fn a_declared_conversation_is_read_as_declared() {
    // Given — opencode.yaml declaring a minted conversation with a deadline
    let yaml = opencode_yaml_declaring(
        "    conversation: { kind: minted, adapter: cursor-create-chat, mint_deadline_secs: 7 }\n",
    );

    // When
    let conversation = the_spec_in("opencode", &yaml).conversation();

    // Then
    assert_eq!(
        conversation,
        ConversationSlot::Minted {
            adapter: "cursor-create-chat".to_string(),
            mint_deadline_secs: Some(7),
        }
    );
}

#[test]
fn a_conversation_adapter_the_build_does_not_hold_is_refused_at_load_naming_the_file_and_the_word()
{
    // Given
    let yaml =
        opencode_yaml_declaring("    conversation: { kind: minted, adapter: no-such-minter }\n");

    // When
    let refused = loading_opencode_as(&yaml).expect_err("an unknown adapter must not load");

    // Then
    assert_eq!(
        (
            refused.contains("opencode.yaml"),
            refused.contains(
                "conversation.adapter no-such-minter is not a launch adapter this build has"
            ),
        ),
        (true, true),
        "the refusal names the file and the word: {refused}"
    );
}

#[test]
fn a_resume_token_conversation_without_a_resume_slot_is_refused_at_load() {
    // Given — opencode.yaml has no `resume` slot to read a token with
    let yaml = opencode_yaml_declaring("    conversation: { kind: resume_token }\n");

    // When
    let refused = loading_opencode_as(&yaml).expect_err("a token nothing reads must not load");

    // Then
    assert!(
        refused.contains(
            "conversation kind resume_token needs a resume.adapter to read its token with"
        ),
        "the refusal names the missing slot: {refused}"
    );
}

#[test]
fn an_unknown_conversation_kind_is_refused_at_load() {
    // Given
    let yaml = opencode_yaml_declaring("    conversation: { kind: telepathy }\n");

    // When
    let refused = loading_opencode_as(&yaml).expect_err("an unknown kind must not load");

    // Then
    assert!(
        refused.contains("telepathy"),
        "the refusal names the word: {refused}"
    );
}
