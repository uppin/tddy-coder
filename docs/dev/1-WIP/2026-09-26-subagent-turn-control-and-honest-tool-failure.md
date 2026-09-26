# Changeset: Subagent Turn Control and Honest Tool Failure

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix + Feature

## Initial Discovery

[2026-09-26-subagent-turn-control-and-honest-tool-failure-initial-discovery.md](./2026-09-26-subagent-turn-control-and-honest-tool-failure-initial-discovery.md)
— three passes: live incident forensics from `~/.tddy`, the subagent MCP and RPC surface, and the
workspace jail lifecycle.

## Prerequisites

Items in `packages/*/docs/code-issues/` and [`docs/dev/todo/`](../todo/) this change runs into.

**No claimed issue is in this change's path.** The two `Claimed by:` records repo-wide
(`tddy-index-daemon`, `tddy-workflow-recipes`) name unrelated code, so there is no
proceed / wait / narrow fork to put to the developer.

### ⚠ DURING — oversized file: `workspace_tool_sandbox.rs` — `packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-workspace-tool-sandbox.md`

590 production lines against a 500 budget, unclaimed, with a designed `extract_module --to_file`
seam that passes a plain `check` but has never been proven by `check --deep`. This change **adds**
to the file (a typed transport-failure outcome on the `WorkspaceSandbox` trait), so it makes the
finding worse.

**Recorded, not fixed here**, by the developer's decision: absorbing a ~263-line mechanical
extraction would bury a behaviour change under it, and the seam's line numbers are already stale
(the record says to re-derive them with `restructure anchors`). Append a measurement row at wrap.

### ⚠ DURING — oversized files in three more packages this change edits

Filed by this change's own Step 2b, because the packages had never been analyzed:

- `packages/tddy-tools/docs/code-issues/oversized-file-server.md` — **2,488** production lines
- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` — **1,208**, and **no
  `#[cfg(test)]` module at all**
- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent-runtime.md` — **557**
- `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` — **793**

All four grow under this change. Same verdict and same reason: recorded so the next Step 2b sees
them weighed, not restructured, so this stays one reviewable PR. `tddy-discovery`,
`tddy-tool-engine` and `tddy-daemon-sandbox` have **never** had the CRAP pipeline run against them —
these records measure file length only, and "not measured" is not "clean".

The `subagent.rs` record carries the sharper half: its `dispatch_tool_call` error shaping is
private, untested and about to become load-bearing. This change builds the unit coverage for that
seam because it has no choice; the rest of the file stays uncovered.

### ⚠ DURING → partly ✅ — The daemon orphans sandboxed runners on shutdown, and never notices one that died — [`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)

The entry's §2 is exactly this change's subject. It names `exchange_in_jail_tool_call`'s
*"it did not answer within {}s"*, the `*guard = None` permanence, and quotes the comment that is
right about why — and concludes *"There is no crash detector, no restart policy, no backoff."*

This change closes the **restart policy** for the workspace tool jail: a dead channel is torn down,
re-provisioned and retried once. It does **not** close §1 (shutdown orphaning), does not add a
crash *detector* (death is still discovered per call rather than watched for), and does not touch
the claude-cli jail family the entry also covers.

**So the entry is narrowed, not deleted.** `/update-context-docs` must edit it to state that the
per-call restart landed here and that shutdown ordering and the `child.wait()` watcher remain open,
and the verdict stays ⚠ DURING. Deleting a partly-fixed entry is the one failure mode the
deferred-work policy names explicitly.

One factual correction to record on the entry while editing it: it points at
`relaunch_sandboxed_runner` as the existing-but-unreachable relaunch. That function serves the
**claude-cli** jail (`sandbox_manager`) and never touches `workspace_sandboxes`. The workspace
jail's relaunch primitive is `JailedWorkspaceSandboxProvisioner::provision`, already wrapped
idempotently by `reprovision_colocated_checkout_jail`.

### ⚠ DURING — `in_jail_conversation_acceptance.rs` runs on no machine anyone has — [`2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)

This is the constraint that shapes the whole testing plan. The suite that would be the natural
real-jail proof is macOS-gated while every CI job is `runs-on: ubuntu-*`, so on CI it compiles to
an empty binary; on macOS it fails in setup about three runs in four on a stdio-bridge attach race.
The entry is right that it proves nothing.

**Recorded, not fixed here** — the entry itself says closing it means fixing the stdio-bridge attach
race first, which is its own change. The consequence for this one: **the gate is the ungated
`tddy-sandbox-runner` relay suites plus new unit and integration coverage.** A Seatbelt-backed proof
may be added but cannot be relied on.

One correction to record on the entry: it implies the file lives under `tddy-daemon/tests/`; it is
`packages/tddy-daemon-rpc/tests/in_jail_conversation_acceptance.rs:28`.

### ℹ ANSWERED — `subagent_status` cannot wait for a turn to end — [`2026-08-29-subagent-status-cannot-wait-for-a-turn-to-end.md`](../todo/2026-08-29-subagent-status-cannot-wait-for-a-turn-to-end.md)

The entry wants `waitFor: "idle"` so a main agent that fired a prompt can rejoin without polling.
Adjacent to `subagent_resume`, and the question it raises is answered by this change's shape:
resume is a **new turn on an existing conversation**, so it queues and reports `queuePosition` /
`queueSize` exactly like a prompt. A caller therefore still has no way to park until a turn ends —
`waitFor: "idle"` remains genuinely open and this change does not make it harder to add.

**Recorded, not fixed here.** Verdict stays ℹ; the entry is not deleted at wrap.

### ℹ ANSWERED — The action tools are advertised where nothing implements them — [`2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md`](../todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md)

Already **Resolved** and due for deletion by its own wrap, but it is the direct precedent for this
change's `Read` defect and is worth naming rather than rediscovering. Same shape — a tool
advertising a capability nothing implements — and the same consequence it records: *"In session
`01a03066`, asked to invoke a specialized agent, the main agent reached for `invoke_action`, got
`unknown tool: InvokeAction`, and reported that the agent was not registered — a wrong conclusion
drawn from a tool that should not have been on offer."*

That is the 2026-09-26 incident in miniature, eleven months earlier. It is also why
`packages/tddy-tools/tests/mcp_tool_advertisement_audit.rs` exists, and that test is what a seventh
`subagent_*` tool must be added to.

### ℹ ANSWERED — two `docs/dev/1-WIP/` changesets are merged but never wrapped

Not TODO entries, but Step 2's "check `1-WIP/` for conflicting active changesets" found them and
they look like conflicts:

- `2026-09-20-specialized-agent-context-handoff.md` — shipped in **#521**, merged 2026-09-20
- `2026-09-20-subagent-turn-queue-visibility.md` — same PR

Both edit the exact files this change edits. Both are **already in master** (`StopReason::ContextExhausted`
and `context_exhausted_outcome` in `subagent.rs`; `queuePosition` / `queueSize` at
`tddy-tools/src/server.rs:1692-1704`). **No conflict.** Their wrap is somebody else's hygiene task,
not this change's work, but a planner reading `1-WIP/` will hit them again.

## Affected Packages

- **tddy-discovery**: no README — the subagent session, its turn loop, message history and tool
  dispatch. The largest share of this change. Newly-filed code issues under
  `packages/tddy-discovery/docs/code-issues/`
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — shell spawn
  hardening and the `Read` line window
- **tddy-daemon-sandbox**: no README — the `WorkspaceSandbox` boundary gains a typed transport
  failure
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) —
  `LocalExecTools` gains the provisioner and the relaunch-and-retry
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — the `subagent_*` MCP
  surface: `maxTurns`, `subagent_resume`, message ids in every outcome
- **tddy-session-agents**: [README.md](../../../packages/tddy-session-agents/README.md) — the
  conversation service serves the budget, the resume and the ids
- **tddy-service**: no README — `session_agents.proto` and the in-jail relay allowlist
- **tddy-sandbox-recipes**: no README — the sandboxed-Claude tool allowlist
- **tddy-sandbox-runner**: no README — no production change expected; its ungated relay suites are
  where the jail-channel behaviour is proven

## Related Feature Documentation

- [PRD-2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../ft/coder/1-WIP/PRD-2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
- [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)
- [discovery-agent.md](../../ft/coder/discovery-agent.md)
- [session-agent-roster.md](../../ft/daemon/session-agent-roster.md)
- [sandboxed-codebase-mode.md](../../ft/daemon/sandboxed-codebase-mode.md)
- [remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md)

## Summary

Give the jail boundary and the subagent's tool dispatch a typed transport-failure signal, so "the
channel is dead" stops being indistinguishable from "the command exited non-zero". On that signal:
rebuild a dead jail once, and refuse to ask a model to summarise a search in which nothing
succeeded. Then hand the main agent control of a subagent's turn budget — `maxTurns` per call, and
a new `subagent_resume` that continues a conversation, optionally rewound to a named message and
corrected. Two defects on the same path are fixed rather than stepped around: a jailed shell
inherits the runner's IPC stdin, and `Read` ignores the `offset`/`limit` it advertises.

## Background

See the PRD for the product framing and the
[initial discovery](./2026-09-26-subagent-turn-control-and-honest-tool-failure-initial-discovery.md)
for the forensics. The technical short version:

`tokio::process::Command::output()` sets stdout and stderr but — unlike `std` — leaves **stdin
inherited**. A jailed `Shell` call whose command read stdin therefore became a rival reader on
`tddy-sandbox-runner --stdio`'s tool-IPC request pipe. The Shell tool's timeout dropped the future
without `kill_on_drop`, so the orphan outlived it. The next tool call blocked on the
one-outstanding-call mutex; 600 s later the in-jail deadline fired and the channel was marked
permanently closed, correctly. Every subsequent tool call in that session — 54 of them, all from
FastContext — was refused.

FastContext burned its ten turns on refused calls and was then handed the synthesis prompt, which
asks for a summary *"citing the specific file:line locations you found"* with no check that
anything was found. At `temperature: 0.0` it produced the same fabricated answer twice, to the byte.

The three decisions that compounded are each defensible alone: a dead channel is reported like a
tool failure *deliberately*; a spent budget lands softly *deliberately*; a broken jail stays broken
*deliberately*, because the alternative is running a jailed session's tools on the host. What was
missing is the distinction between a failure that is the tool's and one that is the channel's.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **PRD documentation**: [PRD-2026-09-26-…](../../ft/coder/1-WIP/PRD-2026-09-26-subagent-turn-control-and-honest-tool-failure.md) created and approved ✅
- [x] **Changeset**: this document created ✅
- [x] **Typed transport failure**: both the `WorkspaceSandbox` boundary and the subagent's tool
      dispatch distinguish a channel failure from a tool that ran and failed
- [x] **Shell containment**: no spawned shell inherits the runner's stdin; a timed-out Shell leaves
      no surviving descendant
- [x] **Jail relaunch**: a dead channel is torn down, re-provisioned and retried exactly once
- [x] **Honest failure**: an all-tools-failed prompt errors without a synthesis model call, and
      leaves the conversation resumable
- [x] **`Read` windowing**: the engine honours the `offset`/`limit` it advertises
- [x] **Turn budget**: `maxTurns` on the MCP tool and the conversation RPC, clamped to 50
- [x] **Resume and rewind**: message ids on every turn outcome; `subagent_resume` with
      `fromMessageId` and `correction`
- [ ] **Package Documentation**: READMEs and package docs updated
- [x] **Testing**: all acceptance tests passing
- [x] **Integration**: cross-package integration verified (MCP ↔ RPC ↔ jail)
- [ ] **Technical Debt**: production readiness gaps addressed
- [x] **Code Quality**: linting, formatting and review complete

## Technical Changes

### State A (Current)

**Shell spawning.** Three sites construct `tokio::process::Command` with neither
`.stdin(Stdio::null())` nor `.kill_on_drop(true)`:
`tddy-tool-engine/src/lib.rs:128-135` (`ShellTaskBody`, background jobs), `:523-528` (the blocking
path), `shell.rs:88-95` (`LocalShell::run`). On timeout, `tokio::time::timeout` drops the future
and the child survives.

**The jail boundary.** `WorkspaceSandbox::execute_tool` returns `ExecuteToolResponse`. A dead
channel produces `ExecuteToolResponse { is_error: true, error_message: "session {id}: the tool call
could not be run in its jail ({reason}); refusing to run it on the host worktree instead" }`
(`workspace_tool_sandbox.rs:435-444`) — **shaped identically to a command that exited non-zero**,
by an explicit decision documented at `:41-46`. `channel: Mutex<Option<InJailChannel>>` is set to
`None` on any exchange error and never restored (`:404`, `:432-434`). `exchange_in_jail_tool_call`
is bounded by `IN_JAIL_TOOL_TIMEOUT = 600s` (`tddy-sandbox-runner/src/host_relay.rs:276`).

**Dispatch to the jail.** `ExecToolRpcHandler` (`tddy-daemon-rpc/src/exec_tool.rs:20-27`) holds no
sandbox registry. `LocalExecTools` (`local_exec_tools.rs:25-29`) holds
`workspace_sandboxes: Arc<WorkspaceSandboxRegistry>` but no provisioner, and dispatches at `:119`.
Three entries reach it: the unary and streaming RPC ports, and `svc_start_hosted_agent_clone.rs:281`
— a roster agent's own turn loop, which is the FastContext path.

**The subagent's tool dispatch.** `dispatch_tool_call` (`tddy-discovery/src/subagent.rs:532-590`)
returns `String`. Success is `value.to_string()`; failure is
`format!("{{\"error\": \"{e}\"}}")` — no `is_error` flag, and the error text is interpolated into
JSON unescaped. `run_one_turn` (`:864-910`) pushes the result with `ChatMessage::tool_result` and
cannot tell the two apart.

**The turn loop.** `SpecializedSubagentSession::prompt` (`:1100-1149`) runs
`for _turn in 0..self.max_turns`, then **unconditionally** calls `run_synthesis_turn` (`:1056-1095`),
which splices in *"Summarize your findings now from what you have already read, citing the specific
file:line locations you found."* and pushes the model's answer onto `self.messages`. Requests are
sent at `temperature: 0.0` (`:615`).

**Turn budget.** `max_turns` reaches the session only from the agent definition
(`agent_def.rs:132-133`, default 10) via `SubagentRegistry::create` (`subagent.rs:1200`). Registry
assistants bypass the def file: `assistant_def.rs:53` hardwires `DEFAULT_MAX_TURNS = 10`, and the
`assistant` table has no such column. `server.rs:1755-1759` states that a roster entry carries no
turn budget deliberately.

**Message history.** `messages: Vec<ChatMessage>` with no ids. The only read surface is
`SubagentSession::tail(max_messages) -> Vec<String>`, which `RemoteAgentSession` returns empty from
(`roster/conversation.rs:291-297`, with a standing TODO).

**Where conversations actually run.** `subagent_new_session` (`server.rs:1760`) runs the loop
in-process only when this process holds the def. The jail holds none — the daemon logs
`spawn seed carries 0 specialized agent def(s)` — so **every conversation is a `RemoteAgentSession`**
relayed over `OpenAgentConversation` / `PromptAgentConversation`.
`PromptAgentConversationRequest` carries `{session_token, session_id, daemon_instance_id,
conversation_id, prompt}`; `AgentConversationChunk` carries `{content_chunk, stop_reason, last}`.

**`Read`.** `catalog.rs:21` advertises `offset` and `limit`. `tool_read` (`lib.rs:302-320`) reads
`path` only. `CodebaseAccess::read_window` forwards both on the Managed path (`subagent.rs:195-206`)
and applies `window_content` only on the Local path, so `DEFAULT_READ_LINE_CAP = 200` is **not
enforced for a jailed subagent at all**.

**`TurnEnd`.** Its `Err` arm sets `took_a_turn: false` because *"a failure spent tokens but added
nothing to the history"* (`subagent_runtime.rs:495-497`) — already false, since the loop pushes as
it runs.

### State B (Target)

**A transport failure is a distinct kind, end to end.** `WorkspaceSandbox::execute_tool` reports
channel death as a typed outcome separate from `ExecuteToolResponse { is_error }`. The subagent's
`dispatch_tool_call` likewise returns a result that says whether the tool *ran*, with its error
payload built by `serde_json` rather than `format!`. A command exiting non-zero is **not** a
transport failure on either surface.

**No spawned shell can reach the runner's stdin,** and a Shell call that exceeds its budget leaves
no descendant: the child is spawned in its own process group and the group is signalled on timeout.

**A dead jail is rebuilt once.** On a transport failure and only then, `LocalExecTools` removes the
session's entry (dropping the last `Arc` tears the jail down via `Drop`), re-provisions through a
new `Arc<dyn WorkspaceSandboxProvisioner>` field, re-inserts, and retries the call exactly once.
A relaunch failure, or a second transport failure, is reported to the caller. The retry is
serialised per session so two concurrent calls cannot relaunch twice. **A jailed session's tools
are still never run on the host worktree**, on any path.

**A prompt in which no tool call succeeded is an error.** The turn loop tracks whether any dispatch
reported the tool as having run. If none did when the budget is spent, `prompt` returns
`Err(SubagentError)` naming the transport failure — **no synthesis turn and no model call**. The
conversation is untouched and remains resumable: `messages` keeps everything the failed prompt
appended. `TurnEnd`'s `took_a_turn` premise is corrected to match.

**`Read` honours its window.** `tool_read` applies `offset` (0-based) and `limit` and returns
`{content, truncated, total_lines}`, the same shape `window_content` already produces, so the
Managed path stops pulling whole files.

**`maxTurns` is the caller's, bounded.** `subagent_prompt` and `subagent_resume` accept an optional
`maxTurns`, clamped to `SUBAGENT_MAX_TURNS_CEILING = 50`. Absent, the def's value applies unchanged.
A clamped value is reported in the outcome so a caller is never silently given less than it asked
for. The budget crosses `PromptAgentConversationRequest`.

**Every turn outcome enumerates the messages it appended**, each with a stable monotonic id, role,
tool name, `isError` and a truncated preview. Ids are minted on append and **never reused**,
including after a rewind. They cross the conversation RPC.

**`subagent_resume` continues a conversation without a new prompt turn.** With `fromMessageId` it
first discards every message after that id, snapping to a legal boundary so a tool-call message
never loses its results. With `correction` it appends exactly one corrective user message after the
rewind point. With neither, it continues from the end. It queues like any turn and reports
`queuePosition` / `queueSize`.

### Delta (What's Changing)

#### tddy-discovery

- **API**: `SubagentSession::prompt` gains a per-call turn budget; new `resume` method taking a
  rewind point and an optional correction; `tail` replaced or joined by an id-bearing history read.
  Both implementors change.
- **Implementation**: `dispatch_tool_call` returns a typed outcome instead of `String`, with
  `serde_json`-built error payloads. The turn loop tracks tool-run success and skips synthesis when
  there was none. Message ids minted on every `messages.push`. Rewind with boundary snapping.
- **Implementation**: `RemoteAgentSession` gains real resume and id support over the RPC; its
  `tail`/usage TODOs are partly answered.
- **Correction**: `TurnEnd::took_a_turn` and its comment.

#### tddy-tool-engine

- **Implementation**: all three shell spawn sites get `.stdin(Stdio::null())`, `.kill_on_drop(true)`
  and own-process-group spawning; the timeout path signals the group. Consolidating the three into
  one helper is the natural way to do it once.
- **API**: `tool_read` honours `offset`/`limit` and returns `{content, truncated, total_lines}`.
  `catalog.rs`'s advertised schema becomes true; its description gains the window semantics.

#### tddy-daemon-sandbox

- **API**: `WorkspaceSandbox::execute_tool` reports a transport failure distinctly. The trait doc at
  `:41-46`, which currently argues for the opposite, is rewritten to say why the distinction now
  exists and what it must not be used for (falling back to the host).

#### tddy-session-lifecycle

- **API**: `LocalExecTools::new` gains an `Arc<dyn WorkspaceSandboxProvisioner>`; its single call
  site at `handler_state.rs:90-96` passes the one `DaemonSessionHost` already owns.
- **Implementation**: `run_exec_tool_locally` relaunches and retries once on a transport failure,
  serialised per session.

#### tddy-tools

- **API**: `subagent_prompt` gains `maxTurns`; new `subagent_resume` tool with its schema, handler
  and router entry; every turn outcome (`prompt`, `await`, `resume`) gains `messages`.
- **Implementation**: `prompt_outcome_json` grows the message list; `DeferredTurn` carries either a
  prompt or a resume.

#### tddy-session-agents

- **Implementation**: the conversation service honours a per-call budget, serves resume, and emits
  message descriptors on the chunk stream.

#### tddy-service

- **API**: `session_agents.proto` — `max_turns` on `PromptAgentConversationRequest`; a resume
  operation; message descriptors on `AgentConversationChunk` or its final frame. `IN_JAIL_RELAYABLE`
  gains the resume operation.

#### tddy-sandbox-recipes

- **Implementation**: `SUBAGENT_TOOLS` (`claude_cli.rs:169-177`) gains
  `mcp__tddy-tools__subagent_resume`, or the tool is advertised and uncallable from a sandboxed
  Claude.

## Implementation Milestones

- [x] **M1 — Typed transport failure, both surfaces.** `WorkspaceSandbox` and `dispatch_tool_call`
      can each say "the channel failed" distinctly from "the tool failed". Error payloads built by
      `serde_json`. Nothing else in this changeset is buildable first.
- [x] **M2 — Shell containment.** stdin nulled, own process group, group killed on timeout, at all
      three sites.
- [x] **M3 — Jail relaunch and retry.** `LocalExecTools` gains the provisioner; tear down,
      re-provision, retry once, serialised per session; host fallback still refused.
- [x] **M4 — Honest failure.** No synthesis when no tool call ran; error names the transport
      failure; conversation stays resumable; `took_a_turn` corrected.
- [x] **M5 — `Read` windowing.** Engine honours `offset`/`limit`; catalog description made true.
- [x] **M6 — Turn budget over the wire.** `maxTurns` on the MCP tools and the proto, clamped to 50,
      clamping reported.
- [x] **M7 — Message ids.** Minted on append, never reused, enumerated in every turn outcome,
      carried over the proto.
- [x] **M8 — `subagent_resume`.** Rewind with boundary snapping, optional correction, fresh budget;
      advertisement audit and sandboxed-Claude allowlist updated in the same commit.
- [ ] **M9 — Documentation.** Feature docs, package docs, and the two TODO entries narrowed.

## Testing Plan

### Testing Strategy

**Determine Appropriate Test Level:**

This changeset spans three layers with genuinely different testable units, so it takes three levels
rather than one:

| Area | Level | Why |
|---|---|---|
| Shell containment, `Read` windowing, id minting, rewind snapping, budget clamping | **Unit / Integration** | deterministic functions and a spawn contract; no session, no jail |
| The subagent turn loop — all-tools-failed, resume, budget | **Integration** | a real `SpecializedSubagentSession` against a `wiremock` provider and an injected `CodebaseAccess::managed`; the contract under test is *what history gets sent to the model* |
| Jail relaunch, and the MCP wire shape | **Acceptance** | relaunch needs a real provisioner boundary; the MCP surface needs the real `--mcp` stdio transport |

**Primary Test Approach:** integration against a `wiremock` provider, because **the strongest
assertion available in this changeset is the request body the model receives.** Every behaviour
that matters — that no synthesis turn was sent, that a rewind actually truncated the history, that
a correction was appended once, that a clamped budget produced exactly N turns — is observable as
the sequence of HTTP requests the provider saw. That is deterministic, needs no timing, and cannot
pass for the wrong reason. `subagent_context_exhaustion_red.rs` already establishes the pattern.

**The constraint that rules out the obvious alternative:** a real-jail end-to-end proof. The suite
that would provide it runs on no CI machine
([`2026-09-12`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)). A Seatbelt
proof can be written but cannot be the gate, so the jail behaviour is proven at the
`WorkspaceSandboxProvisioner` seam with a test double instead.

### Testing Options Analysis

#### Option 1 — Integration against a mock provider, asserting the request sequence (PRIMARY)

**Test Level**: Integration
**Description**: Drive a real `SpecializedSubagentSession` built by `SubagentRegistry::from_defs`
against a `wiremock::MockServer` standing in for the model endpoint, with
`CodebaseAccess::managed(...)` standing in for the tool transport. Assert on the **requests the
provider received**, not only on the returned outcome.

**Scope**:
- Every tool call fails → the budget is spent and **no synthesis request is sent**
- Some tool calls succeed → today's `MaxTurnRequests` synthesis still happens, unchanged
- A resume with `fromMessageId` → the next request's `messages` array is the truncated history
- A resume with `correction` → exactly one extra user message, in the right position
- `maxTurns` → exactly that many requests, and a value above 50 produces 50

**Assertions**:
- [ ] **Request count is exact**: an all-tools-failed prompt with `max_turns: 10` causes the
      provider to receive **exactly 10** requests, never 11. The eleventh would be the synthesis
      turn; its absence is the guarantee, and a count is the only way to see it.
- [ ] **The error names the transport failure**: the returned `Err` contains the injected channel
      error text verbatim, not a generic "turn failed".
- [ ] **The rewound history is exact**: after `resume { fromMessageId: m }`, the next request's
      `messages` equals the prefix through `m` — compared element-wise, not by length.
- [ ] **The correction is one message**: the resumed request's `messages` is the prefix plus
      exactly one `{role: "user", content: <correction>}`, in final position.
- [ ] **Clamping is reported**: `maxTurns: 500` yields 50 provider requests **and** an outcome that
      says the budget was clamped.
- [ ] **The conversation survived**: after the all-tools-failed `Err`, a `resume` on the same
      session sends a request whose `messages` still contains the failed prompt's tool exchanges.

**Reliability Considerations**: fully deterministic — `temperature: 0.0`, a mock provider with
scripted responses, an injected dispatch that returns a fixed error. No sleeps, no real network, no
filesystem race. `wiremock` request recording is exact.

**Implementation Location**: `packages/tddy-discovery/tests/`

#### Option 2 — Acceptance over the real `--mcp` stdio wire

**Test Level**: Acceptance (E2E for the tool contract)
**Description**: Spawn `tddy-tools --mcp` as a real subprocess and exercise `subagent_resume`,
`maxTurns` and the `messages` field over JSON-RPC, as `subagent_mcp_acceptance.rs` and
`subagent_async_response_acceptance.rs` already do for the existing tools.

**Scope**: the advertised schema, the wire shape of the new fields, argument refusals, and the
interaction with the existing grace / `responseId` contract.

**Assertions**:
- [ ] **`subagent_resume` is advertised** with its full schema, and only when the roster has an
      agent — the same gating every other subagent tool obeys.
- [ ] **A turn outcome carries `messages`** with `id`, `role`, `isError` and a truncated `preview`,
      and the preview of a large payload is **shorter than the payload**.
- [ ] **An unknown `sessionId` or `fromMessageId` is an in-band error result**, never a silent
      continue and never a protocol error.

**Trade-offs**:
- **Pros**: proves the thing a main agent actually calls, including schema and gating.
- **Cons**: cannot reach the jail, and cannot cheaply assert what the *model* received — so it
  complements Option 1 rather than replacing it.

#### Option 3 — Unit tests at the two seams that have none today

**Test Level**: Unit
**Description**: In-crate `#[cfg(test)]` coverage for the functions this change makes
load-bearing. `subagent.rs` has **no test module at all** today, so one is created.

**Scope**:
- `dispatch_tool_call`'s error shaping: a tool error carrying `"` or a newline produces **valid
  JSON** (today it does not), and a transport failure is distinguishable from a tool failure
- `tool_read`'s window: offset, limit, past-end, exact-fit, and `truncated` / `total_lines`
- message id minting: monotonic, and an id discarded by a rewind is never minted again
- rewind boundary snapping: a `fromMessageId` landing between a tool-call message and its results
  snaps or refuses, never truncates between them
- `maxTurns` clamping arithmetic

**Assertions**:
- [ ] **The error payload parses**: `serde_json::from_str` succeeds on a tool error whose message
      contains a quote and a newline.
- [ ] **Ids are never reused**: after a rewind discarding `m5..m9`, the next appended message's id
      is `m10` or later, never `m5`.

**Note**: unit tests complement but do not replace the integration and acceptance tests.

#### Option 4 — Jail relaunch at the provisioner seam

**Test Level**: Integration
**Description**: A test `WorkspaceSandboxProvisioner` and a test `WorkspaceSandbox` whose
`execute_tool` returns a transport failure on demand, driven through
`LocalExecTools::run_exec_tool_locally`. No real jail, no Seatbelt, runs on Linux CI.

**Scope**: the relaunch decision, its exactness, and the invariant it must not break.

**Assertions**:
- [ ] **A transport failure relaunches exactly once**: the test provisioner's `provision` is called
      **twice** in total (once at setup, once on the failure) and the tool call succeeds on the
      retry.
- [ ] **A tool that ran and failed does not relaunch**: a non-zero exit leaves `provision` call
      count at **one**.
- [ ] **A second transport failure is reported, not retried again**: `provision` call count is
      **two**, and the caller receives the error.
- [ ] **The old jail is stopped**: the replaced `WorkspaceSandbox`'s `stop()` is observed before
      the new one serves a call.
- [ ] **No host fallback, ever**: on a relaunch failure the response still refuses, and the worktree
      is never touched — asserted by a provisioner that always fails plus a canary file that must
      not be read.

**Trade-offs**:
- **Pros**: runs on CI, exercises the real dispatch layer and the real registry, and pins the
  discrimination that the whole relaunch rests on.
- **Cons**: does not prove a real `tddy-sandbox-runner` comes back up. That gap is named in
  Technical Debt and is the same gap [`2026-09-12`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)
  already records.

### Testing Principles Applied

- **Exact counts over presence checks.** "No synthesis turn happened" is asserted as *exactly 10
  requests*, not as "the content looks like an error". A presence check would pass if synthesis ran
  and happened to fail.
- **Compare histories element-wise.** A rewind assertion that checks `messages.len()` would pass on
  a truncation that removed the wrong messages.
- **The absence of a call is the contract.** Both headline behaviours — no synthesis, no relaunch on
  an ordinary failure — are about something *not* happening, so both are asserted as call counts on
  a recording double.
- **No timing in the deterministic tests.** The only time-based assertions are the two shell
  containment tests, which are inherently about a process budget; both are written so the failure
  mode is a missing side effect rather than a race.

### Coverage Requirements

- [ ] **Happy path**: a prompt whose tools work is unchanged in every respect
- [ ] **Error scenarios**: every tool failing; the jail dying mid-session; a relaunch that itself
      fails; an unknown session or message id
- [ ] **Edge cases**: `maxTurns` of 1, and above the ceiling; a rewind to the first message and to
      the last; a rewind landing mid tool-call group; an empty correction; a `Read` window past
      end-of-file
- [ ] **Integration points**: MCP ↔ conversation RPC ↔ subagent loop; exec-tool RPC ↔
      `LocalExecTools` ↔ `WorkspaceSandbox`
- [ ] **Actual effects verification**: the provider's received requests; the provisioner's call
      count; the absence of a surviving process
- [ ] **Side effects**: the conversation remains open and resumable after a failed prompt

### Test Data Strategy

- Provider responses are scripted `wiremock` mounts, ordered with `up_to_n_times`, exactly as
  `subagent_context_exhaustion_red.rs` does.
- The transport-failure string is pinned as the **real** text the jail emits
  (`"session {id}: the tool call could not be run in its jail (its channel is closed); refusing to
  run it on the host worktree instead"`), not an invented one — the same reasoning the context
  refusal constants are pinned under.
- Shell containment tests use a `tempfile::TempDir` marker file per test; no shared mutable state.
- No random values, no wall-clock dependence beyond the two process-budget tests.

## Acceptance Tests

**Written and verified 2026-09-26.** Three placements changed while writing, each recorded under
*Decisions & Trade-offs*: the `tddy-daemon-sandbox` unit test was dropped, the planned in-crate
`subagent.rs` unit tests became integration tests, and the jail-relaunch suite became an in-crate
unit module.

**All 33 pass as of 2026-09-26.** The markers record the state each test was *written* in — 🔴
fails at runtime with a verified reason, ⚪ fails to compile against absent API, ✅ green from the
start as a regression guard — because which kind a test was is what says whether it ever actually
constrained the implementation.

### tddy-tool-engine

`packages/tddy-tool-engine/tests/shell_containment_red.rs`

- [x] 🔴 `a_shell_command_cannot_read_the_parents_standard_input` :221 — AC1.
      *`cat` blocked on an inherited stdin instead of seeing EOF: Shell: timed out after 200ms*
- [x] 🔴 `a_background_shell_job_cannot_read_the_parents_standard_input_either` :249 — AC1 at the
      `ShellTaskBody` site. *the background job never finished; it is still blocked on an
      inherited stdin*
- [x] 🔴 `a_shell_command_that_outlives_its_budget_leaves_no_descendant_running` :290 — AC2,
      process **group**. *a descendant survived the timeout and touched …*

`packages/tddy-tool-engine/tests/read_window_engine_red.rs`

- [x] 🔴 `reading_a_file_window_returns_only_the_requested_lines` :111 — AC9
- [x] 🔴 `a_window_reaching_the_end_of_the_file_is_not_truncated` :125 — AC9 boundary
- [x] 🔴 `a_window_starting_past_the_end_of_the_file_is_empty_rather_than_an_error` :139
- [x] ✅ `reading_without_a_window_returns_the_whole_file_unchanged` :157 — compatibility for every
      existing caller; catches a lines-and-rejoin implementation dropping the trailing newline

### tddy-discovery

`packages/tddy-discovery/tests/subagent_tool_outage_red.rs`

- [x] 🔴 `a_prompt_whose_every_tool_call_failed_reports_the_transport_failure` :172 — AC6
- [x] 🔴 `a_total_tool_outage_never_reaches_a_synthesis_turn` :197 — AC6 structurally.
      **`left: 5, right: 4`** — the synthesis turn provably runs today
- [x] ✅ `a_prompt_with_working_tools_still_lands_in_a_synthesis_summary` :216 — AC8
- [x] ✅ `one_failed_tool_call_among_successful_ones_is_not_an_outage` :243

`packages/tddy-discovery/tests/subagent_message_ids_red.rs` — ⚪ needs `MessageDescriptor`,
`MessageRole`, `MESSAGE_PREVIEW_CHARS`, `PromptOutcome::messages`

- [x] ⚪ `a_turn_outcome_lists_the_messages_it_appended` — AC12
- [x] ⚪ `a_failed_tool_result_is_marked_and_its_error_text_survives_quoting` — AC12; also pins the
      `format!`-built JSON envelope
- [x] ⚪ `a_large_tool_result_is_previewed_rather_than_carried_whole` — AC12
- [x] ⚪ `message_ids_are_unique_across_the_whole_conversation` — AC13

`packages/tddy-discovery/tests/subagent_resume_red.rs` — ⚪ needs `TurnRequest`, `MessageId`,
`SubagentSession::take_turn`

- [x] ⚪ `resuming_continues_the_conversation_without_sending_a_new_prompt_turn` — AC14
- [x] ⚪ `resuming_from_an_earlier_message_discards_everything_after_it` — AC15
- [x] ⚪ `resuming_with_a_correction_appends_exactly_one_instruction_after_the_rewind_point` — AC16
- [x] ⚪ `resuming_from_an_unknown_message_is_refused_rather_than_continued` — AC17
- [x] ⚪ `a_rewind_never_separates_a_tool_call_from_its_result` — AC18
- [x] ⚪ `a_conversation_whose_prompt_failed_can_still_be_rewound_into` — AC7

`packages/tddy-discovery/tests/subagent_turn_budget_red.rs` — ⚪ needs `TurnRequest`,
`SUBAGENT_MAX_TURNS_CEILING`, `PromptOutcome::clamped_max_turns`

- [x] ⚪ `a_caller_supplied_budget_replaces_the_definitions_for_that_call` — AC10
- [x] ⚪ `a_caller_supplied_budget_does_not_outlive_the_call_that_set_it` — AC11
- [x] ⚪ `a_budget_above_the_ceiling_is_clamped_and_the_outcome_says_so` — AC10
- [x] ⚪ `a_budget_within_the_ceiling_is_not_reported_as_clamped` — AC10

### tddy-session-lifecycle

`packages/tddy-session-lifecycle/src/connection_service/jail_relaunch_unit_tests.rs` (in-crate,
declared at `connection_service.rs`) — ⚪ needs `ToolDispatchOutcome`

- [x] ⚪ `a_tool_call_that_hits_a_dead_jail_relaunches_it_once_and_succeeds_on_the_retry` — AC4
- [x] ⚪ `a_tool_that_ran_and_failed_does_not_relaunch_the_jail` — AC3, the discrimination
- [x] ⚪ `a_second_transport_failure_is_reported_rather_than_relaunched_again` — AC4
- [x] ⚪ `the_replaced_jail_is_stopped_when_it_is_rebuilt` — AC4
- [x] ⚪ `a_jailed_sessions_tools_never_reach_the_host_worktree_when_a_relaunch_fails` — AC5

### tddy-tools

`packages/tddy-tools/tests/subagent_resume_mcp_acceptance.rs` — 🔴 all five, over real `--mcp` stdio

- [x] 🔴 `subagent_resume_is_advertised_when_an_agent_is_attached_and_withheld_when_none_is` — AC20;
      also asserts the schema names `fromMessageId`, `correction` and `maxTurns`
- [x] 🔴 `a_turn_outcome_lists_the_messages_the_turn_appended` — AC12 on the wire
- [x] 🔴 `a_large_tool_result_is_previewed_rather_than_returned_whole` — AC12 on the wire
- [x] 🔴 `a_max_turns_above_the_ceiling_is_clamped_and_the_outcome_says_so` — AC10 on the wire
- [x] 🔴 `resuming_an_unknown_conversation_is_an_error_result` — AC17 on the wire

`packages/tddy-tools/tests/mcp_tool_advertisement_audit.rs` — updated, 43/40 → **44/41**

- [x] 🔴 `advertises_all_forty_four_tool_names_where_the_host_serves_the_action_tools`
- [x] 🔴 `advertises_forty_one_tool_names_on_the_daemon_path_which_serves_no_action_tool`
- [x] ✅ `withholds_exactly_the_three_action_tools_where_the_host_does_not_claim_them` — unchanged

### tddy-sandbox-recipes

`packages/tddy-sandbox-recipes/src/claude_cli.rs`

- [x] 🔴 `claude_allowlist_offers_subagent_resume_exactly_where_it_offers_subagent_prompt` — AC20.
      *subagent_resume must be offered exactly where subagent_prompt is*

### Suites this change must update rather than extend

- [x] `packages/tddy-session-lifecycle/src/connection_service/workspace_sandbox_roster_dispatch_unit_tests.rs`
      — its `RecordingSandbox` implements `WorkspaceSandbox::execute_tool -> ExecuteToolResponse`
      and will not compile against the new return type
- [x] `packages/tddy-sandbox-runner/tests/in_jail_tool_dispatch_logging.rs` — its doc asserts the
      invariant this change alters: *"A jail that lets one call pass its budget loses the channel
      for good"*
- [x] `packages/tddy-daemon-sandbox/tests/tool_catalog_sync.rs` — cross-checks the sandbox
      allowlist against the `tddy-tool-engine` catalog, which the `Read` schema change touches

## Test Results

Scoped per package, 2026-09-26. Never a bare `./test`; whole-workspace health is CI's answer.

| Package | Result |
|---|---|
| `tddy-discovery` | **155 passed, 0 failed** (19 binaries) |
| `tddy-tools` | **247 passed, 0 failed** (41 binaries) |
| `tddy-service` | **103 passed, 0 failed** — incl. `unbundle_service_split` 28/28 |
| `tddy-session-agents` | **160 passed, 0 failed** |
| `tddy-session-lifecycle` `--lib` | **144 passed, 0 failed** |
| `tddy-tool-engine` | **34 passed, 0 failed** |
| `tddy-sandbox-recipes` | **44 passed, 0 failed** |
| `tddy-sandbox-runner` | **52 passed, 0 failed** |

`cargo clippy --all-targets -- -D warnings` clean on all eight; `cargo fmt --check` clean.

**Known failures that are not this change**, verified rather than assumed:

- `packages/tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs` does not compile —
  missing `use tddy_sandbox::SandboxHandle`. Untouched by this branch; last changed by **#518**.
  macOS-gated, so CI never builds it, but it blocks a bare `./test -p tddy-daemon-sandbox` on a Mac.
- Four `tddy-session-lifecycle` acceptance targets panic with *"sandbox RPC bridge not installed"*.
  All four reference **none** of `ToolDispatchOutcome`, `LocalExecTools` or `workspace_sandbox`,
  were last touched at `35cf2913` (two commits before this branch), and this branch's diff never
  mentions `install_sandbox_rpc_bridge`. Not proved against a pristine tree — a `git stash` would
  have risked the working copy, and a clean worktree build was judged too expensive.
- `session_room_acceptance` (Docker port in use) and `session_sync_livekit_acceptance`
  (`tddy-remote-git-repo` not built — `./test` builds binaries first, raw `cargo test` does not)
  are environmental.

## Technical Debt & Production Readiness

- [ ] **Nothing proves the resume crosses the wire.** `ResumeAgentConversation` is
      compile-checked and clippy-clean but otherwise unexercised: the suite that would drive it
      end to end is the one that runs on no machine
      ([`2026-09-12`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)). Cover is
      `tddy-discovery`'s local-session tests plus the MCP acceptance tests, either side of a gap
- [ ] `RemoteAgentSession` refuses one shape the local session supports — a turn carrying **both**
      a new prompt and a rewind point or correction. No MCP caller can construct it, so a wire
      shape was not invented for it; it becomes real work only if `subagent_prompt` ever gains a
      rewind
- [ ] `IN_JAIL_RELAYABLE` widened 5 → 6. Minimal and signed off, but the jail's relay surface grew
      and the closed-set guard in `tddy-session-agents/src/lib.rs` is what keeps the next one honest
- [ ] No end-to-end proof that a real `tddy-sandbox-runner` comes back up after a relaunch — the
      seam is tested with a double because the real-jail suite runs on no CI machine
      ([`2026-09-12`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md))
- [ ] Jail death is still discovered per call, not watched for — no `child.wait()` observer, no
      backoff. §2 of [`2026-09-15`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)
      is only partly closed
- [ ] Shutdown still orphans sandboxed runners — §1 of the same entry, untouched
- [ ] Four oversized files grow under this change; all four are recorded, none restructured
- [ ] `tddy-discovery`, `tddy-tool-engine` and `tddy-daemon-sandbox` have never had the CRAP
      pipeline run; only file length is measured
- [ ] `RemoteAgentSession::cumulative_usage` / `context_tokens` still return zero unless the chunk
      stream is extended to carry usage — this change carries message ids but not necessarily usage
- [ ] The FastContext assistant row has an empty `system_prompt` — data, not code, and not fixed
      here, but it contributed to the fabrication's confidence

## Decisions & Trade-offs

- **One PR, not a stack.** The two halves share a prerequisite (the typed transport-failure signal)
  and the incident that motivates both. Splitting would put M1 in a node that changes no behaviour
  — which the `pr-stack` contract calls an invalid node — and would leave the fabrication defect
  shipping a release later than the wedge that triggered it.
- **Hard error, not a soft landing, when no tool call succeeded.** Chosen over a
  `StopReason::ToolsUnavailable` outcome. A soft landing would be consistent with `MaxTurnRequests`
  and `ContextExhausted`, but those two describe a search that *happened* and ran out; this one
  describes a search that never started. The cost is that an error path must now leave a
  well-defined, resumable conversation — made explicit as its own acceptance test rather than left
  incidental. It also means **no new `StopReason`**, which matters: `parse_stop_reason`
  (`roster/conversation.rs:303-317`) treats an unknown spelling as a hard error, so every new
  variant is a two-sided version-coupled change.
- **Rewind carries an optional correction.** A bare rewind cannot change anything: requests go out
  at `temperature: 0.0`, and the incident demonstrated that twice — the same 1470-character answer,
  byte for byte, from the same history. Offering rewind without correction would have shipped a
  feature that silently does nothing.
- **`maxTurns` is clamped to 50 rather than unbounded or refused above the ceiling.**
  `server.rs:1755-1759` deliberately keeps a turn budget off the roster entry so that editing a
  definition cannot change what a running session may do. A caller-set budget is a different thing
  — bounded, per call, and chosen by the agent doing the work — but unbounded it would let a main
  agent spend arbitrary local-model time. Clamping rather than refusing keeps a caller that asks
  for too much working, and reporting the clamp keeps it honest.
- **Relaunch at `LocalExecTools`, not at `JailedWorkspaceSandbox`.** The jail type holds neither
  its spec nor a provisioner, and giving it both would make the `Arc` handed out by
  `registry.get()` mutable in place. `LocalExecTools` is the only layer that holds the registry
  *and* sits beneath all three dispatch entries, including the roster agent's own turn loop — the
  path FastContext used.
- **The 600-second in-jail deadline is left alone.** It is shared by the runner and the daemon with
  a comment saying the two hosts must not disagree, so changing it is a two-sided edit. With the
  wedge prevented and a relaunch in place, the deadline stops being the thing that hurts.
- **`max_turns` does not become a `models.db` column.** A per-call override makes one unnecessary,
  and adding one would mean a DDL migration, a proto field, and three store methods for a value
  the caller now supplies.
- **Three test placements changed while writing the red phase, all toward less widening.**
  (1) The planned `tddy-daemon-sandbox` transport-failure unit test was **dropped**: with no real
  jail reachable on CI it could only have exercised a double of the trait, which proves nothing.
  The discrimination is tested where it is real, in `a_tool_that_ran_and_failed_does_not_relaunch_the_jail`.
  (2) The four planned in-crate `subagent.rs` unit tests became **integration** tests: the error
  shaping and id minting are observable through the new message descriptors, so reaching the
  private `dispatch_tool_call` — and widening it — turned out to be unnecessary. That also means
  `subagent.rs` does **not** gain its first test module here, and its code issue stands unchanged.
  (3) The jail-relaunch suite is an **in-crate unit module** rather than `tests/`, because
  `LocalExecTools::new` is `pub(crate)` and the path under test is the private
  `local_agent_codebase_access` — the same reason `workspace_sandbox_roster_dispatch_unit_tests`
  already lives in-crate.
- **The stdin tests re-exec their own binary.** The first draft ran `cat` directly in the harness
  and **passed against the live defect**: under `cargo test` the harness's own stdin is already at
  end of file, so the command sees EOF whether or not the engine closed it. A test that green-lights
  the bug it exists to catch is worse than no test, so those two cases re-exec with fd 0 bound to an
  open pipe holding data — the condition a `--stdio` runner is permanently in.
- **Four oversized files recorded, not restructured.** `workspace_tool_sandbox.rs` has a designed
  seam and could have been split first, but absorbing a mechanical extraction would bury a
  behaviour change under it — and the seam's line numbers are already stale.

## Refactoring Needed

### From /plan-red (Planning and Red Phase)

- [ ] Three near-identical shell spawn sites (`tddy-tool-engine/src/lib.rs:128`, `:523`,
      `shell.rs:88`) should become one helper while they are all being fixed
- [ ] `TurnEnd::took_a_turn`'s comment states a premise that is already false
- [ ] `dispatch_tool_call` builds JSON with `format!` — replace with `serde_json`

### From /green (Implementation)

### From /validate-changes (Change Validation)

### From /validate-tests (Test Quality)

### From /validate-prod-ready (Production Readiness)

### From /analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes

### /validate-tests

### /validate-prod-ready

### /analyze-clean-code
