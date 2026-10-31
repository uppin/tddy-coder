//! Slot renderers: what one launch of a coding agent adds to its argv and writes to disk, from its
//! [`CodingAgentSpec`]'s declared slots and the launch's tool set.
//!
//! Every renderer is pure except [`instructions`], which reads the files it prepends to. Nothing is
//! written here: the launcher writes [`RenderedSlots::files`] before it spawns the agent.

pub mod cursor;
pub mod hooks;
pub mod instructions;
pub mod mcp;
pub mod tools;

use std::collections::BTreeMap;
use std::path::PathBuf;

use tddy_agent_backend::claude_hooks::HookCommandParams;
use thiserror::Error;

use crate::CodingAgentSpec;

/// The MCP server a launch registers with its agent (`tddy-tools --mcp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerCommand {
    /// The server's name in the registration (`tddy-tools`).
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    /// The server's `env` block; omitted from the registration when empty.
    pub env: BTreeMap<String, String>,
}

/// What one launch fills the slots with.
pub struct SlotInputs<'a> {
    /// The agent's working directory: instruction files are read from it and the hooks adapter's
    /// file is written under it.
    pub workdir: PathBuf,
    /// Where a registration file (the MCP config) is written — a per-session directory.
    pub scratch_dir: PathBuf,
    /// The tool server to register; `None` registers nothing and adds no flag.
    pub mcp_server: Option<McpServerCommand>,
    /// Pre-approved tools, each passed with the `tool_allow` flag, in order.
    pub allowed_tools: Vec<String>,
    /// Withdrawn tools, each passed with the `tool_deny` flag, in order.
    pub denied_tools: Vec<String>,
    /// The MCP tool the agent asks before anything not pre-approved; `None` adds no flag.
    pub permission_prompt_tool: Option<String>,
    /// Prepended once to each declared instruction file that exists; `None` leaves them alone.
    pub preamble: Option<String>,
    /// Passed with the `append_system_prompt` flag; `None` adds no flag.
    pub append_system_prompt_file: Option<PathBuf>,
    /// The session the hooks report for; `None` writes no hooks file. Parameterised by the
    /// session, so a per-tab activity identity is a later parameter rather than a second writer
    /// (`docs/dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md`).
    pub hooks: Option<HookCommandParams<'a>>,
    /// The agent's home on this launch — what `{home}` in a [`crate::McpSlot::Placed`] path
    /// resolves to: the jail's persistent home, or the host's `$HOME` (`#agent-tools 7/12`). `None`
    /// refuses a placed path that names `{home}` ([`SlotError::UnboundHome`]).
    pub home: Option<PathBuf>,
}

/// What a launch adds: argv entries (spliced before the prompt) and files to write, each with its
/// bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedSlots {
    pub argv_extra: Vec<String>,
    pub files: Vec<(PathBuf, Vec<u8>)>,
}

/// Why a slot could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SlotError {
    /// A spec's `hooks.adapter` names no adapter this build has.
    #[error("unknown hooks adapter {name}")]
    UnknownHooksAdapter { name: String },
    /// An instruction file exists but could not be read.
    #[error("instruction file {}: {message}", path.display())]
    Io { path: PathBuf, message: String },
    /// The launch has values for a slot the agent's spec declares `unsupported` — for example a
    /// list of tools to withdraw from an agent that cannot withdraw any. Refused, never dropped.
    #[error("{agent} cannot take {slot}: its spec declares it unsupported")]
    SlotUnsupported { agent: String, slot: String },
    /// The hooks adapter reports over the daemon's agent-tool socket and the launch gave it none.
    /// Refused rather than falling back to `daemon_url`, which on an embedded daemon answers every
    /// report with a 404 the hook swallows (`#agent-tools 7/12`).
    #[error(
        "hooks adapter {adapter} needs the daemon's agent-tool socket and the launch gave none"
    )]
    NoDaemonSocket { adapter: String },
    /// A placed registration path names `{home}` and the launch bound no home.
    #[error("{path} names {{home}} and the launch bound no home")]
    UnboundHome { path: String },
}

/// Every slot of `spec` filled from `inputs`. `argv_extra` is ordered: the append-system-prompt
/// flag, the allow flags, the deny flags, the permission-prompt flag, the MCP flag — the order the
/// hand-built launch used.
pub fn render(spec: &CodingAgentSpec, inputs: &SlotInputs<'_>) -> Result<RenderedSlots, SlotError> {
    // TODO(claude-host): implement — compose `instructions`, `tools`, `mcp` and `hooks`.
    // TODO(cursor-cli): and the Cursor slots — an `McpSlot::Placed` registration
    // (`cursor::render_cursor_mcp_json` at `cursor::placed_path`), `instructions.rules_file`
    // (`cursor::render_rules_file` with the preamble), and hooks through
    // `HooksAdapter::try_render`, so `cursor-hooks-json`'s refusal without a socket surfaces here.
    let _ = (spec, inputs);
    todo!("claude-host: slots::render")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;
    use tddy_actions::tool::{ArgvItem, RunSection, Transport};
    use tddy_agent_backend::claude_hooks::{build_claude_hooks_settings, HookCommandParams};

    use super::*;
    use crate::{
        CodingAgentSection, FlagSlot, HooksSlot, InstructionsSlot, McpFormat, McpSlot,
        ModelsSection, ToolSlot, Unsupported,
    };

    const PREAMBLE: &str = "## Managed Codebase\n\nThe real codebase is managed.";

    fn flag(flag: &str) -> FlagSlot {
        FlagSlot {
            flag: flag.to_string(),
        }
    }

    /// The slots `specs/claude.yaml` declares.
    fn a_claude_spec() -> CodingAgentSpec {
        CodingAgentSpec {
            id: "claude".to_string(),
            run: RunSection {
                transport: Transport::Pty,
                argv: vec![ArgvItem::Lit("{binary}".to_string())],
                env: BTreeMap::new(),
            },
            agent: CodingAgentSection {
                mcp: McpSlot::File {
                    flag: "--mcp-config".to_string(),
                    format: McpFormat::ClaudeMcpJson,
                },
                tool_allow: ToolSlot::Flag(flag("--allowedTools")),
                tool_deny: ToolSlot::Flag(flag("--disallowedTools")),
                permission_prompt: ToolSlot::Flag(flag("--permission-prompt-tool")),
                instructions: InstructionsSlot {
                    files: vec![PathBuf::from("CLAUDE.md"), PathBuf::from("AGENTS.md")],
                    append_system_prompt: Some(flag("--append-system-prompt-file")),
                    rules_file: None,
                },
                hooks: HooksSlot {
                    adapter: "claude-settings-local".to_string(),
                },
                models: ModelsSection {
                    default: "opus".to_string(),
                    curated: vec!["opus".to_string()],
                },
                context_globs: vec!["CLAUDE.md".to_string()],
                headless: None,
            },
        }
    }

    /// Inputs that fill no slot, for `workdir` and `scratch_dir`.
    fn no_inputs<'a>(workdir: &Path, scratch_dir: &Path) -> SlotInputs<'a> {
        SlotInputs {
            workdir: workdir.to_path_buf(),
            scratch_dir: scratch_dir.to_path_buf(),
            mcp_server: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            permission_prompt_tool: None,
            preamble: None,
            append_system_prompt_file: None,
            hooks: None,
            home: None,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_mcp_file_has_the_claude_mcp_json_shape_for_the_given_server_command() {
        // Given
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let inputs = SlotInputs {
            mcp_server: Some(McpServerCommand {
                name: "tddy-tools".to_string(),
                command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
                args: strings(&["--mcp"]),
                env: BTreeMap::from([("RUST_LOG".to_string(), "info".to_string())]),
            }),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        let config_path = scratch.path().join("claude-mcp-config.json");
        let expected_config = serde_json::json!({
            "mcpServers": {
                "tddy-tools": {
                    "command": "/opt/tddy/bin/tddy-tools",
                    "args": ["--mcp"],
                    "env": { "RUST_LOG": "info" },
                }
            }
        });
        assert_eq!(
            rendered,
            Ok(RenderedSlots {
                argv_extra: vec![
                    "--mcp-config".to_string(),
                    config_path.to_string_lossy().into_owned(),
                ],
                files: vec![(config_path, expected_config.to_string().into_bytes())],
            })
        );
    }

    #[test]
    fn allow_deny_and_permission_prompt_render_one_flag_before_each_value_in_order() {
        // Given
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let inputs = SlotInputs {
            allowed_tools: strings(&["mcp__tddy-tools__Read", "AskUserQuestion"]),
            denied_tools: strings(&["Bash", "Edit"]),
            permission_prompt_tool: Some("mcp__tddy-tools__approval_prompt".to_string()),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        assert_eq!(
            rendered,
            Ok(RenderedSlots {
                argv_extra: strings(&[
                    "--allowedTools",
                    "mcp__tddy-tools__Read",
                    "--allowedTools",
                    "AskUserQuestion",
                    "--disallowedTools",
                    "Bash",
                    "--disallowedTools",
                    "Edit",
                    "--permission-prompt-tool",
                    "mcp__tddy-tools__approval_prompt",
                ]),
                files: Vec::new(),
            })
        );
    }

    #[test]
    fn the_append_system_prompt_file_renders_its_flag_before_the_tool_flags() {
        // Given
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let inputs = SlotInputs {
            append_system_prompt_file: Some(PathBuf::from(
                "/sessions/0190-a/agent-system-prompt.md",
            )),
            allowed_tools: strings(&["Read"]),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        assert_eq!(
            rendered.map(|slots| slots.argv_extra),
            Ok(strings(&[
                "--append-system-prompt-file",
                "/sessions/0190-a/agent-system-prompt.md",
                "--allowedTools",
                "Read",
            ]))
        );
    }

    #[test]
    fn the_preamble_is_prepended_to_each_declared_instruction_file_that_exists() {
        // Given — CLAUDE.md exists, AGENTS.md does not
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        std::fs::write(workdir.path().join("CLAUDE.md"), "Original instructions.\n").unwrap();
        let inputs = SlotInputs {
            preamble: Some(PREAMBLE.to_string()),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        assert_eq!(
            rendered,
            Ok(RenderedSlots {
                argv_extra: Vec::new(),
                files: vec![(
                    workdir.path().join("CLAUDE.md"),
                    format!("{PREAMBLE}\n\nOriginal instructions.\n").into_bytes(),
                )],
            })
        );
    }

    #[test]
    fn rendering_the_instructions_over_their_own_output_prepends_nothing_more() {
        // Given — a CLAUDE.md a previous launch already composed
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let composed = format!("{PREAMBLE}\n\nOriginal instructions.\n");
        std::fs::write(workdir.path().join("CLAUDE.md"), &composed).unwrap();
        let inputs = SlotInputs {
            preamble: Some(PREAMBLE.to_string()),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        assert_eq!(
            rendered.map(|slots| slots.files),
            Ok(vec![(
                workdir.path().join("CLAUDE.md"),
                composed.into_bytes()
            )])
        );
    }

    #[test]
    fn the_hooks_slot_writes_the_settings_its_adapter_renders_for_the_session() {
        // Given
        let workdir = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let params = HookCommandParams {
            tddy_tools_path: "/opt/tddy/bin/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "0190-a",
            os_user: "dev",
            hook_token: "token-1",
            daemon_socket: None,
            answer_wait_secs: 300,
        };
        let expected = serde_json::to_string_pretty(&build_claude_hooks_settings(&params)).unwrap();
        let inputs = SlotInputs {
            hooks: Some(params),
            ..no_inputs(workdir.path(), scratch.path())
        };

        // When
        let rendered = render(&a_claude_spec(), &inputs);

        // Then
        assert_eq!(
            rendered.map(|slots| slots.files),
            Ok(vec![(
                workdir.path().join(".claude").join("settings.local.json"),
                expected.into_bytes(),
            )])
        );
    }

    /// An agent that cannot withdraw its native tools declares `tool_deny: unsupported`; a launch
    /// that asks it to withdraw some anyway is refused by name, never rendered without them.
    #[test]
    fn denied_tools_for_an_agent_whose_tool_deny_is_unsupported_are_refused() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let mut spec = a_claude_spec();
        spec.agent.tool_deny = ToolSlot::Unsupported(Unsupported::Unsupported);
        let mut inputs = no_inputs(dir.path(), dir.path());
        inputs.denied_tools = strings(&["Bash"]);

        // When
        let refused = render(&spec, &inputs).unwrap_err();

        // Then
        assert_eq!(
            refused,
            SlotError::SlotUnsupported {
                agent: "claude".to_string(),
                slot: "tool_deny".to_string(),
            }
        );
    }

    /// The positive control: with nothing to withdraw, an `unsupported` deny slot renders no flag.
    #[test]
    fn an_unsupported_tool_deny_with_nothing_to_withdraw_renders_no_deny_flag() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let mut spec = a_claude_spec();
        spec.agent.tool_deny = ToolSlot::Unsupported(Unsupported::Unsupported);

        // When
        let rendered = render(&spec, &no_inputs(dir.path(), dir.path())).unwrap();

        // Then
        assert_eq!(rendered.argv_extra, Vec::<String>::new());
    }
}
