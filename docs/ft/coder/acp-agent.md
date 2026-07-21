# tddy-coder as an ACP Agent (`--acp`)

**Status:** shipped (`tddy-coder --acp` agent + reusable `AgentChat`/`useAgentChat`) · **Area:** coder ·
**Related:** [acp-protobuf-rpc](acp-protobuf-rpc.md) (the ACP protobuf mirror over LiveKit),
[codex-acp-backend](codex-acp-backend.md), [activity-log-streaming](activity-log-streaming.md),
[session-drawer § Agent Chat](../web/session-drawer.md#agent-chat)

## Problem

Today tddy-coder is only ever an ACP **client** — it drives external coding agents
(`claude-agent-acp`, `codex-acp`) as one of six interchangeable `CodingBackend`s. Its own TDD
workflow is not itself addressable over any standard protocol: the daemon spawns
`tddy-coder --daemon --grpc` and speaks the bespoke `TddyRemote` protocol to it over LiveKit.

Two consequences:

1. **The workflow can't be driven by a standard ACP host.** An editor like Zed, or any script
   that speaks ACP, cannot run a tddy TDD workflow — there is no ACP agent surface.
2. **The agent boundary inside our own path is not uniform.** The daemon↔coder integration is a
   custom protocol, not the same ACP the coding backends already speak.

## Goal

Expose the tddy `WorkflowEngine` as a standard **ACP agent** (`tddy-coder --acp`), and dogfood it:
our own session-host drives `tddy-coder --acp` over ACP and bridges its ACP notifications back to
the web's existing `TddyRemote` stream. The browser chat is unchanged; the agent boundary becomes
ACP end-to-end through our own path.

```
web AgentChat  --TddyRemote / LiveKit-->  session-host (ACP host + bridge)  --ACP / stdio-->  tddy-coder --acp
                        (web wire protocol unchanged)          (translates both directions)      (WorkflowEngine)
```

Coding backends stay **additive** — all six remain selectable via `--agent`; ACP is the default
path used inside the workflow agent. Nothing is removed.

## What a caller sees over ACP

`tddy-coder --acp` speaks ACP v1 over stdio (JSON-RPC 2.0), the same SDK the backends already use.

| ACP method | tddy behavior |
|---|---|
| `initialize` | Advertises `load_session: true` and the workflow agent identity; advertises the coding models available for the selected `--agent` (via `acp_models_from_session_state` / `BackendModels`). |
| `session/new { cwd }` | Allocates a session dir under `cwd`, selects recipe + coding backend, builds a `WorkflowEngine`. Returns a fresh `SessionId`. |
| `session/load` | Resumes an existing workflow session (`SessionMode::Resume`). |
| `session/prompt` | Feeds the prompt into the workflow run; streams progress out as `session/update` (see below). Returns `PromptResponse { stopReason }`. |
| `session/request_permission` | Raised for every point the workflow blocks on the operator: a `ClarificationQuestion` (select / multi-select), a `DocumentApproval`, or a `WorktreeConfirmation`. The caller's chosen option drives the workflow forward. |
| `session/cancel` | Interrupts the running workflow (existing SIGINT / child-kill path). |

### Outbound `session/update` mapping

The workflow's internal events become ACP session updates, so a caller reconstructs the same
picture the TUI shows:

| Internal event | ACP `SessionUpdate` |
|---|---|
| `PresenterEvent::AgentOutput(text)` | `AgentMessageChunk` (streamed token deltas) |
| `PresenterEvent::ActivityLogged` / `ProgressEvent::ToolUse` | `ToolCall` |
| `PresenterEvent::GoalStarted` / plan updates | `Plan` |
| `ProgressEvent::TaskStarted` / `TaskProgress` | `ToolCall` progress |

### Stop reasons

| `ExecutionStatus` | `StopReason` |
|---|---|
| `Completed` | `EndTurn` |
| `WaitingForInput` (answered via a permission round-trip) | turn continues; `EndTurn` on completion |
| `Error(_)` | surfaced as the prompt error; caller sees the failure message |
| cancelled | `Cancelled` |

### Artifacts

PRD / TODO and other workflow artifacts continue to be written to `session_dir/artifacts/`, and are
**also** surfaced as `session/update` content so a purely-ACP caller need not read the filesystem.

## The session-host bridge (dogfooding)

The per-session host process keeps serving the web its unchanged `TddyRemote.Stream` over LiveKit,
but no longer runs the `WorkflowEngine` in-process. Instead it:

- spawns `tddy-coder --acp` as its agent child and drives it via the unified ACP client;
- translates inbound `TddyRemote` `ClientMessage` intents into ACP `prompt` / permission responses;
- translates outbound ACP `session/update` into `TddyRemote` `ServerMessage` events
  (`AgentOutput`, `ActivityLogged`, `ModeChanged` for a permission→select, `WorkflowComplete`).

This reuses the existing `TddyRemoteService` + Presenter view-adapter surface for the web side and
the existing ACP-client machinery for the agent side. The browser is unaffected.

## Non-goals

- Changing the web wire protocol. The browser keeps `TddyRemote`; ACP is inserted only at the
  host↔agent boundary.
- Removing the non-ACP coding backends (`claude` NDJSON, `cursor`, `codex` CLI). They remain
  selectable; ACP is the default.
- A multiplexed single-process host driving multiple agents — one agent child per session.
