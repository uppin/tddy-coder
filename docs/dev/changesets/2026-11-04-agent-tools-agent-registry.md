# 2026-11-04 — One coding-agent registry; agent gates become spec capabilities

**Type:** Feature (breaking configuration change)

`#agent-tools` 12/19 ([#801](https://github.com/uppin/tddy-coder/pull/801)). Packages:
`tddy-coding-agent-specs`, `tddy-daemon-kernel`, `tddy-service`, `tddy-daemon-rpc`,
`tddy-session-activity`, `tddy-session-lifecycle`, `tddy-agent-launch`, `tddy-agent-backend`,
`tddy-worktree-service`, `tddy-coder`, `tddy-web` (plus fixture wiring in `tddy-daemon`,
`tddy-session-split` and `tddy-telegram-control`).

- `tddy-coding-agent-specs`: `CodingAgentRegistry` (`load`, `from_tools`, `builtin`, `empty`, `all`,
  `get`, `for_session_type` by the `<id>-cli` convention), `Capability` (8 variants, closed),
  `ProviderKind`, `RouteShape`, `RouteSupport`; `coding_agent.{capabilities, answer_wait_secs}` and
  `route.{providers, shape}` on every built-in spec.
- `tddy-daemon-kernel`: `refuse_removed_agent_keys` / `RemovedAgentConfigError`, checked by
  `DaemonConfig::load` on the untyped document.
- `tddy-service` / `tddy-daemon-rpc`: `AgentInfo.{kind, harness, route_providers}` from the registry;
  the project-home notify tool leaves the `operator_notify` gate to the activity service.
- `tddy-session-activity`: `ActivityServiceImpl::with_agents`; hook reports, operator notifications and
  question answers gate on `hooks`, `operator_notify` and `ask_user_question`;
  `worktree_removal_applies_to(agents, st)` reads `keep_worktree_on_delete`.
- `tddy-agent-launch`: `HostCodingAgents::{try_new, agents}`; `cli_launcher` / `CliLauncher` pick the
  launch path from the registry; `split_host` and `requires_model` read from the spec; an ssh session's
  hook slot files are written on the ssh target.
- `tddy-worktree-service`: `BranchIntentPolicy::cli_agent(st, branch_name_required)`.
- `tddy-coder`: `agent_registry_cli::{backend_agent_ids, backend_selection_question_from}`.
- `tddy-web`: the provider picker requests routes only when `route_providers` is non-empty.
- Fixtures: `test_util::{builtin_coding_agents, daemon_config_from_fixture_file}` and
  `DaemonSessionHost::with_coding_agents_if_none`; the source scan `no_agent_name_gates.rs` pins that no
  gate in the session crates compares a session type with an agent's name.

Backlog: resolves `2026-10-22-tool-session-provider-picker-needs-harness-metadata`. Adds
`2026-11-04-provider-route-harness-enum-remains` and `2026-11-04-spec-for-session-type-is-unused`
(`route-support`), `2026-11-04-claude-and-cursor-launch-by-agent-id` (`cli-spec-features`,
`cli-dispatch-collapse`), `2026-11-04-removed-agent-config-readers-remain` (`spec-backend`,
`agent-config-removal`) and `2026-11-04-telegram-delete-leaves-agent-worktrees` (`agent-config-removal`).

## Moved to later nodes (developer, 2026-11-04)

Branches `feature/agent-tools/<name>`, planned on top of this node:

- **n13 `route-support` ([#819](https://github.com/uppin/tddy-coder/pull/819))**: route types into `tddy-agent-backend::provider_route`, `provider_route::Harness`
  removed, routes decided by `RouteSupport`; `spec_for_session_type` deleted.
- **n14 `cli-spec-features` ([#820](https://github.com/uppin/tddy-coder/pull/820))**: claude and cursor features on the generic CLI spec path (managed workflow,
  hooks, slots, conversation id, ssh, tmux via `plan_host_pty`).
- **n15 `headless-driver` ([#821](https://github.com/uppin/tddy-coder/pull/821))**: one generic headless driver from the spec replaces the bespoke headless backends.
- **n16 `spec-backend` ([#822](https://github.com/uppin/tddy-coder/pull/822))**: `SpecBackend{Headless,Acp}` replaces `AnyBackend`; `tddy-coder --agent <any id>`;
  `BUILTIN_BACKEND_AGENT_IDS` and `create_backend` go.
- **n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823))**: the `CliLauncher` / `CLAUDE_AGENT_ID` / `CURSOR_AGENT_ID` dispatch and the
  hand-built paths go; branch intent from the spec; child spawns through specs.
- **n18 `telegram-spec-launch` ([#824](https://github.com/uppin/tddy-coder/pull/824))**: Telegram spawn and pickers from the specs.
- **n19 `agent-config-removal` ([#825](https://github.com/uppin/tddy-coder/pull/825))**: the config types and readers; `answer_wait_secs` at its call sites; fixture
  migration off `daemon_config_from_fixture_file`; worktree removal on a Telegram delete.

## Decisions

- **Breaking removal, refused loudly**: the daemon names the replacement rather than ignoring the key.
- **Capabilities are a closed Rust enum**: an unknown word in YAML is a load error.
- `ProviderKind` distinguishes `openai_responses` from `openai_chat` (Codex vs OpenCode), not one
  `openai_compatible`.
- `for_session_type` resolves `<id>-cli` for any PTY agent, so `codex-cli` and `oz-cli` resolve too.

## Code issues

Measured by the `/pr-wrap` file-length gate against `origin/feature/agent-tools/acp-agents`; not split
(restructuring deferred to the `#agent-tools` sweep node by the developer): new records
`oversized-file-provider-route` (492 → 502), `oversized-file-claude-cli-spawn-steps` (534 → 537),
`oversized-file-svc-start-session-core` (497 → 502); regressed `oversized-file-spec` (656 → 711),
`oversized-file-telegram-session-control-chaining-and-listing` (518 → 522),
`oversized-file-create-session-pane` (524 → 526), `complexity-svc-start-session-core-start-session-core`
(`start_here` 343 → 348).
