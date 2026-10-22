# OpenCode ACP backend (`--agent opencode`)

**Product area:** Coder  
**Status:** Current  

## Summary

**tddy-coder** supports **`--agent opencode`**: a **`CodingBackend`** that runs **`opencode acp`** as a
stdio subprocess and speaks the **Agent Client Protocol (ACP)** to it, the way **`claude-acp`** drives
`claude-agent-acp` and **`codex-acp`** drives `codex-acp`. It is the headless, tool-session form of
OpenCode; the interactive OpenCode TUI is the **`opencode-cli`** terminal session
([OpenCode CLI session](../daemon/opencode-cli-session.md)).

## Selecting it

- **`--agent opencode`** selects **`OpenCodeAcpBackend`**. In the interactive backend choice it is the
  seventh option, **OpenCode**, after every other backend.
- **Binary:** `--opencode-cli-path`, `TDDY_OPENCODE_CLI`, coder-config `opencode_cli_path`, else
  **`opencode`** on `PATH` (`OpenCodeAcpBackend::DEFAULT_CLI_BINARY`). It is the same binary the
  `opencode-cli` harness runs, so one override names both.
- **Models:** the default is **`fireworks/kimi-k2`**, an OpenCode `provider/model` id. `tddy-tools
  list-models --agent opencode [--opencode-cli-path <PATH>]` asks the agent: the models of a new ACP
  session. The daemon's model probe for an `opencode` tool session passes the binary it resolved for
  the `opencode-cli` harness.
- **Allowed agents:** `opencode` is listed in `allowed_agents` of the dev and desktop daemon
  configurations. It is a built-in backend id, so a specialized assistant cannot be named `opencode`.

## Behavior

- **Worker:** a dedicated thread with a `tokio::LocalSet`, started on the backend's first command, so
  a provider route set after construction reaches the agent. `initialize` once, then `new_session`
  (or `load_session` on a resume), then `prompt` with the merged system and user prompt.
- **Progress:** agent message chunks accumulate into the turn's output; message chunks, tool calls,
  terminal tool states and plan entries reach the `ProgressSink` — the same mapping every ACP backend
  uses.
- **Permissions:** every request is approved once (`allow-once`); the run is headless.

## Running on a provider account

On a provider account the tool session's route file ([provider accounts § Tool
sessions](../daemon/provider-accounts.md#tool-sessions)) is applied to the agent child:

- The route's generated `opencode.json` is written owner-only (`0600`) into a fresh owner-only
  (`0700`) directory under the temporary directory, and `OPENCODE_CONFIG` names it. The directory is
  removed when the worker ends. A route file name that is not a bare file name is refused, so no route
  writes outside that directory.
- The account's key is in the child's environment (`TDDY_OPENCODE_PROVIDER_KEY`), which the
  configuration reads; it is never an argument.
- The route's `--model tddy-route/<model>` argument is **not** passed to `opencode acp`: the generated
  configuration already selects that model. Whether `opencode acp` accepts `--model` was not checked
  against a real OpenCode — every test runs `tddy-acp-stub` or a recording script in its place.

## Tests

`packages/tddy-integration-tests/tests/opencode_acp_backend.rs` — a turn and model listing against
`tddy-acp-stub` (which advertises one model, `stub-model`, in every new session), `resolve_agent_models`
with the binary override, and a routed agent's environment and configuration file.
`packages/tddy-agent-backend/tests/opencode_registration.rs` — the appended selection entry and the
`provider/model` default.

## Related documentation

- [Coder overview](1-OVERVIEW.md) — backend selection
- [Codex ACP backend](codex-acp-backend.md) — the same worker model over `codex-acp`
- [OpenCode CLI session](../daemon/opencode-cli-session.md) — the OpenCode TUI and its route
- [Provider accounts](../daemon/provider-accounts.md) — the capability matrix and tool-session routes
- Implementation: **`packages/tddy-agent-backend/src/backend/opencode_acp.rs`**
