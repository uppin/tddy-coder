# 2026-11-04 — The daemon.yaml agent sections are refused, but their types and readers remain

**Category:** Deferred removal
**Source:** `#agent-tools` 12/12 (#801), `TODO(agent-registry)` at `packages/tddy-agent-launch/src/harness_from_spec.rs`,
`packages/tddy-daemon-kernel/src/config/agent_cli.rs`, `packages/tddy-coder/src/run.rs`

## What is there

`DaemonConfig::load` refuses `claude_cli:`, `cursor_cli:` and `cli_harnesses:` naming the tool file
that replaces them. A config built another way (`serde_yaml::from_str`, a struct literal) still has
the `claude_cli`, `cursor_cli` and `cli_harnesses` fields, `ClaudeCliConfig`, `CursorCliConfig`,
`CliHarnessConfig`, and about 30 readers (`resolve_claude_binary_path`, `resolve_cursor_*`,
`claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`,
`claude_hook_daemon_url`) that are, after a load, always at their defaults. The environment overrides
(`TDDY_CLAUDE_BINARY`, …) still work.

The spec side is only partly there: `coding_agent.answer_wait_secs` exists and reads through
`CodingAgentSpec::answer_wait_secs`, but the hook and `AwaitQuestionAnswer` call sites still use the
kernel's default; the fullscreen renderer variable and the persistent-home key are not yet declared in
`claude.yaml`. `tddy-coder`'s `BUILTIN_BACKEND_AGENT_IDS`, `create_backend` and `AnyBackend` still
decide which `--agent` ids can be built: `backend_agent_ids` (registry) lists what *could* be offered,
and a registry id with no backend adapter (a dropped-in agent) is not buildable until a spec-plus-adapter
backend exists. `cli_harness` descriptors (`TODO(harness-clis)`) carry a `requires_model` copy for the
legacy start-session schema. A Telegram-initiated delete no longer removes an agent session's worktree
(`delete_session_directory` takes `None` there: that controller has no registry).

## What to decide

Delete the config types and readers, route the answer wait and the renderer env through the spec at every
call site, give `tddy-coder` a spec-plus-adapter backend, and hand the Telegram controller the registry.

**Changeset ACs left open:** "Removed: `AnyBackend`, `BUILTIN_BACKEND_AGENT_IDS`, the daemon.yaml agent
sections' types"; `tddy-coder --agent` accepting an arbitrary registry id (the *listing* is done).
