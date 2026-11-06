# Initial discovery — agent-tools sweep (YAML-only agent metadata)

State A distilled from grep/LSP-free caller tracing on 2026-11-06 at `37ad1f30b`
(`#agent-tools 19/19`). The sweep node (n20) named by
`docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md`,
`docs/dev/todo/2026-11-05-callerless-bespoke-launch-helpers.md` and
`docs/dev/todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md`.

## The legacy table and its production consumers

`packages/tddy-agent-backend/src/cli_harness.rs` (208 lines) plus
`cli_harness_muse.rs` / `cli_harness_warp.rs` hold, per agent: session type,
label, context agent, default binary, binary env, route config env, argv fn,
model source, `requires_model`. Every field already has a YAML home:
`specs/<id>.yaml` (`binary`, `run.argv`, `coding_agent.route.config_env`,
`coding_agent.models`, `coding_agent.context_globs`, `capabilities`
(`requires_model`), `product`, `CodingAgentRegistry::for_session_type`).

Production consumers of `CliHarness` / `CliLaunch` / `cli_harness()` /
`cli_harnesses()` / `CliModelSource` (excluding `#[cfg(test)]` and test files):

| Consumer | Use | Spec replacement |
|---|---|---|
| `tddy-agent-launch/src/facilitation.rs:33` | `cli_harness(t).is_some()` facilitated check | registry `for_session_type` |
| `tddy-agent-launch/src/session_coordinate_handlers.rs:190` | same | same (via facilitation) |
| `tddy-session-files/src/context_files.rs:98` | descriptor's `context_agent` | spec `context_globs` + spec id |
| `tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs:274,302` | `(harness.argv)(&CliLaunch{..})` for an agent terminal tab | `plan_host_pty`-style render of spec `run.argv` |
| `tddy-daemon-rpc/src/catalog/ports.rs:215` | `ListedByBinary` model-source check | spec `models.list_command`/`adapter` |
| `tddy-daemon-rpc/src/catalog/agent_models.rs` | TODO marker | spec models |
| `tddy-tool-engine/src/start_session_args/schema.rs:46,59` | session-type enum + requires-model list | builtin registry: session types, labels (`product`), `Capability::RequiresModel` |
| `tddy-cli-sessions/src/cli_session_manager/argv.rs:10` | `registered_argv` via descriptor | render spec `run.argv` |
| `tddy-cli-sessions/src/cli_session_manager/launch.rs:41` | `launch_cli_harness` (callerless) | delete |
| `tddy-agent-backend/src/backend/model_catalog.rs:101` | harness model catalogue / `list_by_binary` | caller-supplied registry (specs crate owns listing) |
| `tddy-agent-launch/src/opencode_cli_spawn.rs:115` | `harness_binary(&CliHarness)` | resolved tool binary |
| `tddy-agent-launch/src/cli_harness_jail.rs:383+` | legacy `--agent-kind` helpers (`#[allow(dead_code)]`) | delete with their pinned tests |
| `tddy-agent-launch/src/start_request_checks.rs:90` | `refuse_an_unsupported_cli_harness_start` (legacy refusal) | delete; spec refusal remains |

`claude_argv.rs` is used only by `cli_harness.rs`'s `claude_cli_argv`.

## Model catalogue and context globs

- `backend/mod.rs:590` `curated_models_for_agent` matches agent ids; `:611`
  `claude_cli_models`, `:622` `cursor_cli_models` — the lists already live in
  `specs/claude.yaml` / `specs/cursor.yaml` `coding_agent.models.curated`.
  Labels in YAML are ids today (label == id in the JSON contract for curated
  spec models — verify against the golden tests before deleting).
- `backend/context_globs.rs:35` `context_globs_for_agent(agent)` matches agent
  ids; every row is `coding_agent.context_globs` in the specs (claude, cursor,
  codex, opencode, muse, warp verified). Its caller is
  `CodingBackend::context_globs` (`backend/mod.rs:725`) plus
  `tddy-session-files`, `tddy-session-split`, `tddy-session-lifecycle`.

## Callerless / bespoke leftovers

- `tddy-agent-launch/src/claude_cli_spawn.rs:33` `create_routed_session_dir`
  (`#[allow(dead_code)]`, `TODO(cli-dispatch-collapse)`).
- `tddy-agent-launch/src/relaunch_jail_steps.rs`, `cli_harness_jail.rs` below
  the `--agent-kind` line, `svc_relaunch_sandboxed_runner.rs` Relaunch records
  — see the 2026-11-05 callerless record.
- `tddy-tools/src/pty_relay.rs:107` defaults `session_type` to `"claude-cli"`.
- `tddy-sandbox/src/context_dir.rs` `PREAMBLE_FILES = ["CLAUDE.md", "AGENTS.md"]`
  and `MANAGED_CODEBASE_PREAMBLE` — proposed shape in
  `docs/dev/todo/2026-11-03-tddy-sandbox-still-names-the-guidance-files-of-two-agents.md`:
  callers pass `guidance_files` (from the spec's `instructions.files`).

## Protocol adapters (code, not metadata — stay, named by YAML)

Stream/hook adapters (`claude-stream-json`, `cursor-stream`,
`claude-settings-local`, `cursor-hooks-json`, ACP hooks adapters) are protocol
implementations selected by name from YAML. Their Rust code necessarily knows
the protocol; no agent *metadata* (binary, argv, models, globs, capabilities)
may be read from them after the sweep.

## Code-issue cross-check (Step 2b)

`packages/*/docs/code-issues/` for every touched package was listed; no record
carries `Claimed by:`, so no wait-or-proceed fork applies. Oversized-file
records for `backend/mod.rs`, `cli_harness_jail.rs`, `spec.rs` are noted as
prerequisites with verdict "improved, not closed" — deletions shrink them.
