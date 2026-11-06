# 2026-11-06 — Agent settings come only from specs; the daemon.yaml agent types and readers are deleted

**Type:** Feature (breaking configuration change), with planned and approved deletions

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)), branch
`feature/agent-tools/agent-config-removal`. Based on `feature/agent-tools/telegram-spec-launch`
([#824](https://github.com/uppin/tddy-coder/pull/824)). The last feature node of the stack; the sweep
node (n20) follows. Completes the "daemon.yaml agent sections removed" criterion of the coding-agent
registry PRD. Release note: [daemon changelog](../../ft/daemon/changelog/2026-11-06-agent-tools-daemon-yaml-agent-sections-removed.md).

Packages: `tddy-daemon-kernel`, `tddy-agent-launch`, `tddy-session-activity`,
`tddy-session-lifecycle`, `tddy-session-split`, `tddy-daemon-rpc`, `tddy-telegram-control`,
`tddy-daemon`, `tddy-coding-agent-specs`, `tddy-actions`, `tddy-sandbox-recipes`, `tddy-sandbox-app`,
`tddy-agent-backend` (a comment-only marker move).

## What changed

- **No agent setting is read from daemon.yaml.** `DaemonConfig.{claude_cli, cursor_cli,
  cli_harnesses}`, `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig` and every reader are
  deleted; the kernel has no agent type. `refuse_removed_agent_keys` stays, so a daemon.yaml naming a
  section still fails start-up with each field's replacement — `claude_home_dir` now names
  `home: { kind: path, path: <dir> }` (settled decision).
- **Binaries come from the spec on the daemon's host** (`tddy_agent_launch::host_coding_agents::daemon_tool_host(base)`):
  the spec's `binary.env` on `base`, then `base`'s `$HOME/.local/bin`, then `base`'s `PATH` without
  wrapper-shim directories; `tddy-tools` with no `TDDY_TOOLS_BINARY` is the daemon binary's sibling.
  What none locates is refused by the spec resolution, naming the agent, its `binary.env` and every
  place searched — no "configured value as-is" step. `host_coding_agents(tddy_data_dir)` takes no
  config.
- **The answer wait is per session** (`tddy-session-activity/src/answer_wait.rs`): a launch writes the
  hooks file from `coding_agent.answer_wait_secs` and records the wait in `answer-wait-ms` beside
  `.session.yaml` (`record_answer_wait`); `AwaitQuestionAnswer` holds for
  `session_answer_wait(session_dir, session_type, agents)` — the record, else the spec, else
  `DEFAULT_ANSWER_WAIT_SECS`. `AnswerWaitPorts.wait` is gone. A file, not a `SessionMetadata` field,
  because `.session.yaml` is `deny_unknown_fields`.
- **`claude.yaml` declares `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`** and the out-of-band additions
  on the host, resume, jail and relaunch paths are removed, so an override can turn fullscreen off
  (fixes #801's accepted regression).
- **`home: { kind: path, path: <dir> }`** (`tddy-actions` `HomeSection::Path`): an operator's absolute
  directory, mounted read-write by `lower_home` and named in `tddy-sandbox-app`'s banner; a relative
  path is refused when the tool file loads.
- **`coding_agents` is non-optional** in `LaunchSessions` and `DaemonCodingAgentLauncher`: a host that
  installed none is given an empty registry (`no_coding_agents`, `DaemonSessionHost::launch_coding_agents`),
  whose every agent session is refused for naming no spec. The agentless hand-built ACP launch path is
  deleted.
- **A Telegram delete removes an agent session's worktree again**, passing the controller's registry
  to `delete_session_directory` (`KeepWorktreeOnDelete` honoured).
- **The catalogue's model probe** (`CatalogRpcHandler::listing_binary`) resolves the agent's binary
  through the host's own coding agents (`CatalogAgentSources { tddy_data_dir, model_registry,
  coding_agents }`, keeping the handler within its 5-field budget) instead of reloading the tool files
  per probe, which ignored a host's installed agents.
- **Agent terminal tabs** (`svc_terminal_ports.rs`) and split agents (`SplitSessions.claude_binary`)
  resolve their binaries through the host's coding agents.
- **Fixtures:** every caller of `daemon_config_from_fixture_file` (and of n17's
  `builtin_coding_agents_for`) moved to `DaemonConfig::load` / `test_config()` plus
  `test_util::coding_agents_with_binaries(&[(id, binary)])` — the built-in agents on a host mapping
  each spec's `binary.env` to a stub, with an empty `PATH`. Both helpers are deleted.

## Removed

Planned (changeset § Removed): the three `DaemonConfig` fields, their defaults and re-exports; the
three config types; `resolve_claude_binary_path`, `claude_cli_answer_wait_secs`,
`claude_cli_renderer_env`, `cli_harness_binary`, `resolve_cursor_binary_path`,
`resolve_cursor_cli_tddy_tools_path`, `resolve_cursor_cli_daemon_url`, `resolve_cursor_home_dir`,
`resolve_claude_home_dir`, `daemon_hook_urls::claude_hook_daemon_url`, `CLAUDE_FULLSCREEN_ENV`; the
kernel's `DEFAULT_ANSWER_WAIT_SECS`; `configured_cli_binary`; `DaemonToolHost`'s `config`;
`AnswerWaitPorts.wait`; the agentless ACP path of `DaemonCodingAgentLauncher` (`harness_command`,
`apply_route`'s hand-built branch, `claude_agent_acp_command`, `refuse_acp_when_agents_failed`,
`acp_agents_for`, the `BUNX` / `CLAUDE_AGENT_ACP*` / `CODEX_ACP*` / `OPENCODE_*` constants); the four
out-of-band `CLAUDE_CODE_NO_FLICKER` additions; `test_util::daemon_config_from_fixture_file`; the
kernel tests `claude_cli_config_tests`, `cursor_cli_config_tests` and `tests/answer_wait_secs_config.rs`;
#823's `a_host_without_coding_agents_refuses_…_failed_precondition_spawning_nothing` (a type guarantee
now).

**Beyond the plan, made by green and approved under the stack's "finish without stopping"
instruction (2026-11-05):**

- The kernel's `choose_claude_binary` / `choose_cursor_binary`, `ClaudeBinaryChoice` /
  `CursorBinaryChoice`, `find_real_claude_on_host`, `canonicalize_if_path` and the
  `CLAUDE_BINARY_ENV` / `CURSOR_BINARY_ENV` / `CLAUDE_HOME_ENV` / `CURSOR_HOME_ENV` constants — the
  binary choice now lives in the spec resolution, so the changeset's "keepers" list was wrong about
  `choose_*_binary`.
- `resolve_start_session_claude_binary` / `resolve_resume_session_claude_binary` and their test
  modules `start_session_binary_resolution_tests.rs`, `resume_session_binary_resolution_tests.rs`
  (tests of the removed functions).
- `test_util::builtin_coding_agents_for(config)` (n17's) and n17's interim
  `TDDY_CURSOR_AGENT → resolve_cursor_binary_path` step in `DaemonToolHost`, with its unit test
  `host_coding_agents_unit_tests::cursor_runs_the_binary_daemon_yaml_names_under_cursor_cli` — a
  production branch only fixtures reached, removed at the orchestrator's instruction (2026-11-06).
- n18's fixture host `AFixtureHost` / `coding_agents_launching` in
  `session_agent_remote_acceptance.rs`, replaced by `coding_agents_with_binaries`.

## Parent code changed in this branch

- `tddy-daemon-rpc` acceptance fixtures (14 files): their `DaemonCodingAgentLauncher`s are given
  `builtin_coding_agents()` now that the launcher's agents are not optional, and the `claude_cli:
  binary_path: /bin/cat` fixtures install that stub through `claude.yaml`'s `binary.env`. Green's
  `CliSessionManager::start` → `start_with_options` moves in that crate became identical to n18's own
  (ce750ed90) and left no diff after the rebase.
- `tddy-actions` moved from a dev-dependency to a normal dependency of `tddy-session-lifecycle`
  (`test_util::coding_agents_with_binaries` hosts the fixture binaries behind a `ToolHost`).
- `SplitSessions.claude_binary` (`tddy-session-split`) — a port the host fills with its `claude`
  coding agent's resolution, replacing the split crate's config read.
- `tddy-daemon/src/runtime/telegram_wiring.rs::telegram_coding_agents` (n18's) and
  `opencode_cli_spawn.rs`'s resume (n17's `split_env` kept, `renderer_env` dropped) at rebase.
- `cross_host_stack_parent_acceptance.rs` (n17's) re-pointed from `claude_cli: binary_path` to a stub
  through `claude.yaml`'s `binary.env`.

## Backlog

- `docs/dev/todo/2026-11-04-removed-agent-config-readers-remain.md` — **resolved, deleted.** Final
  state: no field, type or reader of the daemon.yaml agent sections remains; `rg` for
  `ClaudeCliConfig|CursorCliConfig|CliHarnessConfig|daemon_config_from_fixture_file|resolve_claude_binary_path`
  finds only historical comments. (The backend half was n16's, #822.)
- `docs/dev/todo/2026-11-04-telegram-delete-leaves-agent-worktrees.md` — **resolved, deleted.** Final
  state: `handle_delete_session` passes `Some(&self.coding_agents)`; pinned by
  `telegram_session_control_integration::a_telegram_delete_of_a_claude_cli_session_removes_its_worktree_and_of_a_cursor_cli_session_keeps_it`.
- `docs/dev/todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md` — **narrowed, kept.**
  The launcher half is deleted here; `tddy-coding-subagent`'s `withdrawal_session_meta`,
  `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS` and `ACP_HARNESSES`
  remain for the sweep node (n20), since they back `CodingAgentLauncher`'s trait defaults.
- `docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md` — narrowed: the daemon.yaml
  half of the table is gone; the table itself stays for n20.

## Code issues (re-measured at wrap, production lines to the first `#[cfg(test)]`)

| Record | Before → after | Verdict |
|---|---|---|
| `tddy-session-lifecycle` `oversized-file-coding-agent-launcher` | 1119 → 990 | better, still open |
| `tddy-agent-launch` `oversized-file-opencode-cli-spawn` | 685 → 673 | better, still open |
| `tddy-telegram-control` `oversized-file-telegram-session-control-chaining-and-listing` | 522 → 521 | better, still open |
| `tddy-agent-launch` `oversized-file-cli-agent-steps` | 648 → 651 | regressed |
| `tddy-sandbox-app` `oversized-file-main` | 957 → 958 | regressed |
| `tddy-agent-launch` `oversized-file-cli-harness-jail` (new record) | 582 → 591 | first detection |
| `tddy-session-lifecycle` `oversized-file-test-util` (new record) | 501 → 531 | first detection |

No split here: restructuring is deferred to the sweep node (n20) by the developer.

## Tests

New: `answer_wait_unit` (5), `daemon_tool_host_unit` (3), `tool_registry_acceptance::a_path_home_that_is_not_absolute_…`,
`lower_jail_sections_unit::a_path_home_is_the_operators_directory_mounted_read_write`,
`yaml_only_agent_acceptance` (answer wait from an override; edited after start),
`claude_cli_fullscreen_renderer` (no-flicker from `claude.yaml`, host and jail),
`claude_cli_session_acceptance::a_claude_cli_start_whose_binary_is_not_found_is_refused_naming_its_binary_env`,
the Telegram delete test above, and `no_agent_name_gates::the_fixtures_start_every_agent_from_a_daemon_yaml_with_no_agent_section`.
