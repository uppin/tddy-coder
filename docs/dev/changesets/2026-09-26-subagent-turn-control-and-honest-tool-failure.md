# 2026-09-26 — Subagent turn control and honest tool failure

**Type:** Bug Fix + Feature · PR #545

One chain, from a wedged jail channel to a fabricated answer, cut in four places.

## What landed

| Package | Change |
|---|---|
| `tddy-daemon-sandbox` | `WorkspaceSandbox::execute_tool` returns `ToolDispatchOutcome::{Ran, TransportFailed}` — the tool ran and answered, or the call never reached a tool. The trait doc, which previously argued for reporting both alike, states what the distinction is for (whether to rebuild) and what it must never become (a route onto the host worktree) |
| `tddy-tool-engine` | `contained_shell` — one spawn helper for the blocking `Shell`, `ShellTaskBody`, `LocalShell::run` and `Grep`'s `rg`: `/dev/null` stdin, own process group, group signalled on overrun. `Read` honours `offset`/`limit` and returns `{content, truncated, total_lines}` |
| `tddy-session-lifecycle` | `LocalExecTools` gains a `WorkspaceSandboxProvisioner` and `jail_relaunch::JailRelaunch`: on a transport failure only, tear down, re-provision, retry exactly once, serialised per session; host fallback still refused |
| `tddy-discovery` | `SubagentSession::take_turn(TurnRequest)` replaces `prompt`; new `subagent::turn_request` (budget, bounds, rewind, correction) and `subagent::transcript` (ids, previews, boundary-snapping). A budget spent entirely on tool calls of which none ran is an `Err` returned before any model call. `dispatch_tool_call` returns a typed outcome with `serde_json`-built payloads. `RemoteAgentSession` gains resume and ids over the RPC |
| `tddy-service` | `session_agents.proto`: `ResumeAgentConversation`, `max_turns` on prompt and resume, `from_message_id`/`correction` on resume, `AgentMessageDescriptor` and `clamped_max_turns` on the final chunk. `IN_JAIL_RELAYABLE` 5 → 6 |
| `tddy-session-agents` | the conversation service honours a per-call budget, serves resume, and emits message descriptors on the chunk stream |
| `tddy-tools` | `maxTurns` on `subagent_prompt`; new `subagent_resume` with schema, handler and router entry; `messages` on every turn outcome; advertisement audit 43/40 → 44/41 |
| `tddy-sandbox-recipes` | `mcp__tddy-tools__subagent_resume` in the sandboxed-Claude allowlist, pinned to be offered exactly where `subagent_prompt` is |

## Tests

33 acceptance tests, written red and green at delivery. Scoped per package, 2026-09-26:
`tddy-discovery` 155, `tddy-tools` 247, `tddy-service` 103, `tddy-session-agents` 160,
`tddy-session-lifecycle --lib` 144, `tddy-tool-engine` 34, `tddy-sandbox-recipes` 44,
`tddy-sandbox-runner` 52 — all passing, `clippy --all-targets -D warnings` and `fmt --check` clean
on all eight. Whole-workspace health is CI's answer, not a local run's.

The primary level is integration against a `wiremock` provider, because the strongest assertion
available is *the request body the model receives*: "no synthesis turn happened" is an exact
request count, never a content check, and a rewind is compared element-wise, never by length.

Three placements changed while writing the red phase, each toward less widening: the planned
`tddy-daemon-sandbox` transport-failure unit test was dropped (a double of the trait proves
nothing with no real jail on CI), the planned in-crate `subagent.rs` unit tests became integration
tests (the behaviour is observable through the new message descriptors, so reaching a private
function was unnecessary — and `subagent.rs` therefore still has no test module), and the
jail-relaunch suite is an in-crate unit module because `LocalExecTools::new` is `pub(crate)`.

## Code issues reconciled — all open, none closed

No record this change touched went clean, so none was deleted. Measurements as re-run at wrap:

| Record | Measurement |
|---|---|
| `tddy-tools/docs/code-issues/oversized-file-server.md` | 2,652 production lines (2,488 at detection) · budget 500 |
| `tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md` | 2,611 · budget 500 — +1 on this branch, two prose comments and no code |
| `tddy-discovery/docs/code-issues/oversized-file-subagent.md` | 1,431 (from 1,208 at detection) · budget 500 · still **no `#[cfg(test)]` module at all** |
| `tddy-session-agents/docs/code-issues/oversized-file-service.md` | 1,055 (877 at this PR's merge-base) · budget 500 |
| `tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | 789 · budget 500 — **improved** from 793 at detection: `contained_shell.rs` (128) and `read_window.rs` (46) took out more than the fixes put in |
| `tddy-daemon-sandbox/docs/code-issues/oversized-file-workspace-tool-sandbox.md` | 614 (590 at detection) · budget 500 |
| `tddy-discovery/docs/code-issues/oversized-file-subagent-runtime.md` | 595 (557 at detection) · budget 500 |
| `tddy-session-agents/docs/code-issues/complexity-service-take-a-turn.md` | 120 raw / 93 code lines · nesting 5 |
| `tddy-tools/docs/code-issues/complexity-server-take-a-turn.md` | 74 raw / 55 code · nesting 3 |
| `tddy-discovery/docs/code-issues/complexity-subagent-run-turn-loop.md` | 70 raw / 43 code · nesting 3 |

Recorded rather than restructured, by the developer's decision, so this stays one reviewable PR:
absorbing a mechanical extraction of `workspace_tool_sandbox.rs` would have buried a behaviour
change under it, and that record's designed seam has stale line numbers. `tddy-discovery`,
`tddy-tool-engine` and `tddy-daemon-sandbox` have **never** had the CRAP pipeline run against them
— these records measure file length only, and "not measured" is not "clean". Deferral is tracked in
[`docs/dev/todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md).

## Backlog

**No `docs/dev/todo/` entry was resolved, so none was deleted.** Two were edited rather than
removed:

- [`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)
  — **narrowed**, not closed. Its §2 asked for a restart policy and got one for the workspace tool
  jail; §1 (shutdown orphaning), the missing crash *detector* and the claude-cli jail family are
  untouched. Deleting a partly-fixed entry is the failure mode the deferred-work policy names. The
  entry also carried a factual error, corrected in place: `relaunch_sandboxed_runner` serves the
  **claude-cli** jail and never touches `workspace_sandboxes`; the workspace jail's relaunch
  primitive is `JailedWorkspaceSandboxProvisioner::provision`.
- [`2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)
  — re-read and corrected: the file is `packages/tddy-daemon-rpc/tests/in_jail_conversation_acceptance.rs`,
  not under `tddy-daemon/tests/`. Still open, and it is the constraint that shaped the whole
  testing plan.

Seven entries were **filed** by this change, recording what it did not do:
`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it`,
`2026-09-26-a-jail-rebuild-can-re-run-a-tool-call-that-already-executed`,
`2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold`,
`2026-09-26-five-absence-assertions-in-the-new-subagent-suites-have-no-positive-control`,
`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change`,
`2026-09-26-the-honest-failure-guard-does-not-cover-a-model-ended-turn`,
`2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test`.

## Decisions worth keeping

- **One PR, not a stack.** Both halves need the typed transport-failure signal first, and a node
  that only adds it changes no behaviour — which the `pr-stack` contract calls an invalid node.
- **A hard error, not a new `StopReason`.** `MaxTurnRequests` and `ContextExhausted` describe a
  search that happened and ran out; this describes one that never started. It also avoids a new
  variant: `parse_stop_reason` treats an unknown spelling as a hard error, so every variant is a
  two-sided version-coupled change.
- **Clamped, not refused, above the ceiling.** An over-eager caller keeps working, and reporting
  the clamp keeps it honest. A definition's own budget is the operator's and stands unclamped.
- **Relaunch at `LocalExecTools`, not at the jail type**, which holds neither its spec nor a
  provisioner.
- **The 600-second in-jail deadline is untouched** — a two-sided constant shared by the runner and
  the daemon, and with the wedge prevented it stops being the thing that hurts.
- **The stdin tests re-exec their own binary.** The first draft ran `cat` in the harness and passed
  against the live defect: under `cargo test` the harness's own stdin is already at end of file. A
  test that green-lights the bug it exists to catch is worse than no test.

[docs/ft/coder/changelog/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../ft/coder/changelog/2026-09-26-subagent-turn-control-and-honest-tool-failure.md) ·
[docs/ft/daemon/changelog/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../ft/daemon/changelog/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
