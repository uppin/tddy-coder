# 2026-11-04 — The daemon.yaml agent sections are refused, but their types and readers remain

**Category:** Deferred removal
**Owner:** `#agent-tools` n19 `agent-config-removal` ([#825](https://github.com/uppin/tddy-coder/pull/825)) (`feature/agent-tools/agent-config-removal`), and n16
`spec-backend` (`feature/agent-tools/spec-backend`) for the backend half — moved out of #801 by the developer on 2026-11-04, to be planned on top of
`feature/agent-tools/agent-registry`. Code markers: `TODO(agent-config-removal)`.
**Source:** `#agent-tools` 12/19 (#801), `TODO(agent-config-removal)` at `packages/tddy-agent-launch/src/harness_from_spec.rs`
(`configured_cli_binary`), `packages/tddy-agent-backend/src/cli_harness.rs` (`requires_model`),
`packages/tddy-session-lifecycle/src/test_util.rs` (`daemon_config_from_fixture_file`) and
`packages/tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs`; the types are in
`packages/tddy-daemon-kernel/src/config/agent_cli.rs` and `packages/tddy-coder/src/run.rs`

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

## Backend — n16 `spec-backend` ([#822](https://github.com/uppin/tddy-coder/pull/822))

- `SpecBackend{Headless,Acp}` replaces `AnyBackend`; `BUILTIN_BACKEND_AGENT_IDS` and `create_backend` go, so
  `tddy-coder --agent <any registry id>` builds (today `backend_agent_ids` lists what could be offered).

## Config removal — n19 `agent-config-removal` ([#825](https://github.com/uppin/tddy-coder/pull/825))

- Remove `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig`, the `DaemonConfig` fields and their readers.
- Wire `coding_agent.answer_wait_secs` at the hook and `AwaitQuestionAnswer` call sites.
- Migrate the fixtures off `daemon_config_from_fixture_file` (40 files name it on #801's head) and delete it.
- Restore worktree removal on a Telegram-initiated delete (hand the Telegram controller the registry;
  [2026-11-04-telegram-delete-leaves-agent-worktrees.md](2026-11-04-telegram-delete-leaves-agent-worktrees.md)).
- Remove the `TDDY_*_CLI` / `TDDY_*_BINARY` overrides a spec's `binary.env` does not name (PRD *Summary*, breaking).
- Declare the Claude fullscreen renderer in `claude.yaml`'s `run.env` and read it there: today
  `DaemonConfig::claude_cli_renderer_env` always forces `CLAUDE_CODE_NO_FLICKER=1` (a loaded config's
  `claude_cli.fullscreen` is always its default), so a host that needs `fullscreen: false` cannot
  express it until this lands.
