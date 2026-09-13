# 2026-09-09 — The session-agent and activity services

**Type:** Architecture

Node 7 of the `#unbundle` stack ([#476](https://github.com/uppin/tddy-coder/pull/476), 7 of 9, based
on node 6's branch, [#475](https://github.com/uppin/tddy-coder/pull/475)). Seventeen RPCs leave
`connection.ConnectionService` — **50 → 33** — into two new crates, and a known-ambiguous field
numbering is fixed on the way out because the schema it lived in was being replaced anyway.

PRD: [docs/ft/daemon/changelog/2026-09-09-unbundle-session-agent-services.md](../../ft/daemon/changelog/2026-09-09-unbundle-session-agent-services.md).

## The two coordinates

| Coordinate | Family | Methods | Served by |
|---|---|---:|---|
| `session_agents.SessionAgentService` | B | 9 | [`tddy-session-agents`](../../../packages/tddy-session-agents/docs/session-agent-service.md) |
| `activity.ActivityService` | M, N | 8 | [`tddy-session-activity`](../../../packages/tddy-session-activity/docs/activity-service.md), and `tddy-coder`'s session participant for 4 of them |

**`session_agents.SessionAgentService`**: `AttachSessionAgent`, `DetachSessionAgent`,
`ListSessionAgents`, `StreamSessionAgents`, `OpenAgentConversation`, `PromptAgentConversation`,
`CancelAgentConversation`, `ReportAgentCloneState`, `ReportAgentConversationState`.

**`activity.ActivityService`**: `ReportSessionStatus`, `StreamSessionActivity`,
`ReportAgentActivity`, `StreamSessionNotifications`, `StreamAgentActivityDelta`, `StreamAcpReplay`,
`GetAcpToolCallDetail`, `GetAcpReplayPage`.

Both are mounted on the daemon's local Unix socket — `Server::builder()` now takes **six**
`add_service` calls — keeping the stack-wide reachability policy: every family that was reachable
there as part of `connection.ConnectionService` stays reachable after it moves, because dropping one
is a silent capability removal on a privileged interface whose failure mode is a caller that used to
work receiving `unimplemented` with no announcement. Both adapters are **generated** by node 6's
`generate_tonic_adapter`, 17 of 17 methods, nothing stubbed and none hand-written.

`connection.proto` needed no `reserved` field numbers, for the same reason node 6 found: protobuf has
no `reserved` for service *methods*, and nothing that stayed referenced a message that left. A header
note at `connection.proto:12-24` is the standing record of the 22 + 17 vacated coordinates.

One exception, and it is the only genuinely shared type this node found: `SessionAgentStatus` and
`SessionAgentActivity` moved to `types.proto`, because `ListSessions` — family C, which stays —
reaches both through `SessionEntry`, while the roster this node owns produces those rows.
`SessionEntry` keeps field numbers 31 and 32 and points at the shared declarations.

## Five of family B's nine are a security boundary

`packages/tddy-sandbox-runner/src/runner.rs` holds the `(service, method)` tuple allowlist of what an
in-jail agent may relay to its host, and five of its entries are family B. Move the coordinate
without moving the allowlist and every in-jail subagent conversation fails **closed** — the safe
direction, but silently and at runtime rather than at compile time.

The mitigation is that the pairs are now **data**: `tddy_service::session_agents::IN_JAIL_RELAYABLE`
is the one declaration, read by the runner and by `tddy-session-agents`, so the allowlist and the
served coordinate cannot drift. It lives in `tddy-service` rather than in the new crate because its
second reader is the binary running inside every jail, which must not inherit that crate's `livekit`
dependency tree for five string pairs.

The permitted operation set is byte-for-byte the same five operations. Only the service name each
tuple carries changed.

⚠ **And the test that was supposed to prove it runs on no machine anyone has.**
`in_jail_conversation_acceptance.rs` was written for exactly this — a real Seatbelt jail, the
production `dial_and_bridge`, three conversation RPCs over the jail's tool-IPC socket, the coordinate
spelled as a literal so it cannot agree with production by construction. Its
`#![cfg(target_os = "macos")]` is an **inner** attribute, so Ubuntu CI compiles it to an empty test
binary and runs none of it; on macOS it fails in setup about three runs in four on a stdio-bridge
defect its own `FIXME(sandbox-stdio-attach)` records — a predecessor's, demonstrated by the untouched
`sandbox_session_stdio_acceptance` failing identically. No retry was added, deliberately: a
green-looking retry hides it. Recorded in
[`docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md).

What does run is the enforcement half: the runner refuses a non-allowlisted rpc `not_found` without a
frame reaching the host, paired with the positive case so the refusal cannot pass vacuously; and
`tddy-session-agents`' unit tests pin the permitted set and that every tuple names the new service.

## The tick numbering, fixed rather than carried forward

A session's first activity delta was numbered **0**, which is also the wire's "no tick yet" sentinel,
so `tddy-session-sync`'s `decide_record` could not tell *the first delta* from *no delta yet* — and
both LiveKit suites warmed the room before a client attached to work around it.

`StreamAgentActivityDelta` was getting a **new proto** here, and a known ambiguity carried into a
fresh schema becomes permanent. So `session_room.rs` now keeps `last_delta_seq: Option<u64>` and
stamps through `next_tick`; the first delta is `FIRST_TICK` (1) and `NO_TICK` (0) means only what it
says. `next_tick` saturates rather than wrapping, because a wrap would land the next tick back on
`NO_TICK` and reintroduce the exact ambiguity being removed.

The invariant is a **compile-time** assertion — `const _: () = assert!(FIRST_TICK != NO_TICK);` — not
a test. The two tests originally written for it were tautologies: one restated the function's own
`None` arm, the other compared two consts declared four lines apart. The wire-level guarantee stays
where it belongs, in `stream_agent_activity_delta_rpc_acceptance`.

The `docs/dev/todo/` entry that raised it is **closed**.

## Where the constants live is a shipped binary's dependency tree

`NO_TICK`, `FIRST_TICK` and `next_tick` first landed in `tddy-session-activity`. `tddy-session-sync`
is a **standalone installed binary** whose only dependencies were `tddy-livekit` and `tddy-service`,
and reading a `const u64 = 0` from that crate pulled in `teloxide`, `teloxide-core`,
`teloxide-macros`, `tddy-telegram` and `tddy-github`.

They now sit in `tddy_service::session_activity`, beside `ACTIVITY_SERVICE`, which this node had
already put there for the same reason. `cargo tree -p tddy-session-sync | grep -ci teloxide` goes
**3 → 0** and the lockfile diff is only the two removed edges.

The same rule placed `IN_JAIL_RELAYABLE`. Both are cases of one principle: a constant read by a
process that must stay small belongs beside the proto, not beside the server.

## The silent regression, and what it says about fail-quiet clients

`tddy-tools session-hook` still POSTed `ReportSessionStatus` and `ReportAgentActivity` to
`/rpc/connection.ConnectionService/…`, which this node removed both from. The move migrated the
message *types* in that same file and left the URL behind.

It would have failed **invisibly**. The hook swallows every error and exits 0 by contract, and the
command is baked into every Claude and Cursor CLI session's `settings.json` / `hooks.json` — so
session status in `.session.yaml`, the Telegram attention alerts read off it, and every
agent-activity row would simply have stopped, with nothing said anywhere.

Nothing caught it because the existing hook test asserted exit 0 against an *unreachable* daemon —
which the fail-quiet contract makes true in every state, including this one. The replacement
(`session_hook_cli.rs`, 9 tests) serves the real Connect router with `activity.ActivityService`
mounted and nothing else, runs the real binary against it, and asserts on **what the service
received**. The URL now reads `tddy_service::session_activity::ACTIVITY_SERVICE`, so it cannot drift
again.

**The lesson generalises past this node:** a client whose contract is to fail quietly cannot be
covered by a test that asserts its exit code. Every remaining `#unbundle` node should ask which of
its consumers swallow errors, because those are the ones a green suite says nothing about.

## Tests that could not fail, replaced by tests that can

Nine were deleted across the two validation passes. The pattern in each is the same — an assertion
whose subject was not the thing production uses:

| Deleted | Why it could not fail |
|---|---|
| the two socket-mount tests | one read `local_socket_server.rs` as **text** and looked for a type name a bare `use` satisfies with no mount at all; its sibling asserted three filenames are **absent**, which is true in exactly the state the first exists to catch |
| `the_sandbox_relay_allowlist_names_the_new_service` | its `||` was satisfied by two *comments* in `runner.rs`, so deleting `FORWARDED_RPCS` entirely left it green |
| the two entry-builder suites | `build_session_agents_entry` / `build_activity_entry` had **zero** production callers — the daemon builds its own `ServiceEntry` wrapping the `PeerRouted*` decorator the leaf builder knows nothing about — and they asserted `entry.name` against the same hardcoded literal one line away, on an object nothing mounts. ~360 lines of port fakes existed only to feed them |
| the two tick tautologies | restatements of the function's own arm and of two consts in one file |
| `ActivityError` / `SessionAgentError` round-trips | both types were constructed nowhere in production, and their variants (`TruncatedReplay`, `RosterStale`, `NoAddressableAgent`) name refusals this code does not have. One was named for refusing a truncated replay while performing no replay and no refusal |

What replaced them runs over the real socket: `local_token_uds.rs` already mounted both adapters and
never called either, so it now makes one `ListSessionAgents` returning a roster and one
`ReportSessionStatus` whose refusal quotes the status string the request carried — proving the body
reached the implementation behind the adapter. Both were demonstrated failing with the `add_service`
calls removed.

`coder_activity_entry` is now asserted through the public `session_service_entries` seam, so deleting
the registration can no longer leave the coder's replay family dark with every test still green.

## 24 log lines had quietly left the operator's filter

[`docs/dev/todo/2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md`](../todo/2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md)
sets the policy: moved code keeps `target: "tddy_daemon::…"` so an operator's existing `RUST_LOG`
filter keeps matching. Six of this node's moved lines complied. **24 carried no target at all** and so
defaulted to the new crate path, dropping silently out of `RUST_LOG=tddy_daemon=debug`. Each now
names the daemon module it came from, traced against the base tree rather than guessed.

## The cross-host regression the move introduced

The peer daemons in `session_agent_remote_acceptance` joined with `ConnectionServiceServer` alone, so
once the rpcs left, every forward got `Unknown service: session_agents.SessionAgentService`. They now
serve the participant helper that mounts all forwarded coordinates.

It is the same shape node 6 recorded twice: **a mechanical call-site substitution silently changes
which layer answers**, and the failure surfaces in the cross-host suite rather than in the unit one.

## Ports, and the two signatures the plan published that could not work

`build_activity_entry` takes `ActivityPorts` and `build_session_agents_entry` takes
`SessionAgentPorts` — the shape `build_session_files_entry` established in node 6. The signatures
this node's own draft contract published were `AgentActivityHub` and `LiveAgentRoster`, and neither
is the state a server needs:

- `AgentActivityHub` is a per-session broadcast and nothing else.
- `LiveAgentRoster` is the roster **as a client sees it** — seeded from `TDDY_SUBAGENTS_JSON` and
  replaced wholesale by every published frame. It is a subscriber to this service, not its state.
  Handing the service the client's mirror would have `AttachSessionAgent` write into a copy nobody
  persists.

Both are still consumed, unchanged, as fields.

The ports hold `ConnectionServiceImpl` **by value** rather than `Arc<Self>`, avoiding `self_arc()` —
which panics when `set_self_handle` was never called, and would have panicked on all 17 handlers in
every test that constructs the service directly. That is the same harness fault the documented
sandbox `self_arc` failure family comes from.

## What each side kept, and why

- **`daemon_instance_id` peer routing stays in `tddy-daemon`** — `PeerRoutedSessionAgents` and
  `PeerRoutedActivity` wrap the crates' implementations. Seven of the nine family-B methods route on
  the request's named daemon, and that needs the eligible-daemon roster, the common room slot and the
  per-method LiveKit clients. The forward that follows the **agent's** owning daemon *is* inside
  `tddy-session-agents`, because it can only be taken after reading the roster entry that names the
  owner.
- **`report_agent_clone_state` is deliberately not routed.** Its `daemon_instance_id` names the
  *reporting* daemon, not a destination.
- **`session_list_enrichment.rs` stays** (1,740 lines, the largest session module). It enriches
  `ListSessions`, which is family C; taking it would mean taking family C, and the endpoint of this
  stack is a daemon that still owns session lifecycle. Both new crates reach it through ports —
  `SessionLabels` for the notification label, `AgentCatalog` for the roster's def resolution.
- **`TelegramNotificationSubscriber` stays in the daemon**, implementing the moved trait from the far
  side of the crate boundary, which is what the trait is for: it needs `TelegramDaemonHooks`, which
  holds the daemon config, the bot sender and the session watcher.
- **`session_agent_clone.rs` left a 24-line shim behind** for `clone_worktree_path`, which resolves a
  worktree root out of a `workspace` session's own `.session.yaml`. Its module doc originally claimed
  that function is why the file stayed. It is not: **the function has no callers and had none on the
  base either**, and the similarly named
  `ConnectionServiceImpl::agent_clone_worktree_path` is a different function and is what the
  acceptance suites drive. It is left in place — pre-existing dead code is not a move's to delete —
  but the doc now says so.
- **`extern crate self as tddy_service;`** was added to `tddy-service`'s `lib.rs`. Node 6's generator
  emits `tddy_service::to_tonic_status` and the path is deliberately not configurable, so the first
  adapters generated into `tddy-service`'s own `OUT_DIR` do not compile as emitted. Reported upward
  rather than fixed here, per `## Boundaries`:
  [`docs/dev/todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md`](../todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md).

## The second server moved in lockstep, but not the way node 6's did

`tddy-coder`'s session participant serves families M and N for LiveKit-routed sessions, and moved to
`activity.ActivityService` in the same PR for the reason
[`2026-08-02-activities-tail-first-autoscroll`](../../../packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md)
records: the two hosts drifting means one session opens tail-first over HTTP and head-first over
LiveKit. A session on the participant now has **three** coordinates, not two.

Node 6 went further for the terminal family: its participant registers `tddy-terminal-rpc`'s **own**
entry constructor, so there is one implementation and the two servers cannot drift. This node did
not. The participant keeps its own replay handlers, sharing only `tddy-service`'s paging helpers
(`tail_page`, `page_before`, `strip_tool_body`); the framing and the `seq` stamping around them are
written twice, and `two_server_parity_acceptance.rs` is what holds them in line. Recorded in
[`docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md`](../todo/2026-09-12-the-acp-replay-framing-is-written-twice.md).

## Files over 500 lines: the plan named the wrong three

Measured on **production** lines (everything before the first `#[cfg(test)]`):

| File | Prod | Test |
|---|---:|---:|
| `tddy-session-agents/src/session_agent_clone.rs` | **1,157** | 98 |
| `tddy-session-agents/src/service.rs` | **876** | 0 |
| `tddy-session-activity/src/service.rs` | **819** | 0 |
| `tddy-session-agents/src/session_agent_roster.rs` | 394 | 270 |
| `tddy-session-agents/src/session_agent_status.rs` | 304 | 477 |

The changeset's budget line named `session_agent_clone.rs`, `session_agent_status.rs` and
`session_agent_roster.rs` as the three over 500. Only the first is: the other two are majority test
module, `session_agent_status.rs` by more than half. The two files that *are* over budget beside it
are the two **new** `service.rs` files, which no plan anticipated because neither existed when the
budget was written.

None was split. A split for line count alone cuts cohesive units and puts churn on top of a move, and
splitting a file nodes 8 and 9 also touch cascades conflicts through their diffs. Recorded in
[`docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md`](../todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md).

The line count is not the interesting part of `session_agent_clone.rs`. It holds **two daemons' worth
of behaviour** — the facilitating side's bookkeeping and the owning side's LiveKit participant — and
the owning half is the *sole* reason `tddy-session-agents` depends on `tddy-daemon-livekit` and
`livekit`, contradicting `ports.rs`'s own header claim that nothing here reaches for a peer, a room
or a config. The seam and the `CloneTransport` port that would close it are in
[`docs/dev/todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md`](../todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md).

## Test suites: 0 of 14 moved, and one crate has none at all

Not one suite moved with the code. All **sixteen** `tddy-daemon` suites covering these two
subsystems are pinned by `ConnectionServiceImpl` or `test_util::{test_service, TEST_TOKEN}`, and
moving either would put `tddy-daemon` back on the new crates' dependency path and defeat the
extraction — the same measurement node 6 made (5 of 14 movable) and node 4 before it (5 of 18). They
all pass where they are.

The consequence is uneven. `tddy-session-agents` has **47** unit tests in three modules.
**`tddy-session-activity` has zero** — no `tests/` directory, no `#[cfg(test)]` module, across 1,573
production lines.

⚠ And one specific check has **no equivalent anywhere**: nothing asserts that the moved ACP replay
output matches what the old coordinate produced. For a move-only change that is the check that
matters, and `two_server_parity_acceptance` cannot supply it — it compares the two servers as they
are now, so a difference both sides acquired in the move is invisible to it. Recorded honestly rather
than papered over:
[`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).

## The wider consumer fan-out, and the one predicted consumer that was not one

| Consumer | What changed |
|---|---|
| `tddy-sandbox-runner` | the relay allowlist reads `IN_JAIL_RELAYABLE` |
| `tddy-tools` | `session-hook`'s URL, the roster and conversation clients, `mcp_primitives`, `server` |
| `tddy-discovery` | `roster/conversation.rs`, `roster/registry.rs`, `roster/stream.rs`, `subagent_runtime.rs` — the coordinate read from `tddy_service::session_agents::SESSION_AGENT_SERVICE` |
| `tddy-session-sync` | `StreamAgentActivityDelta`'s coordinate and `NO_TICK` instead of a literal `0` |
| `tddy-coder` | the participant's families M and N |
| `tddy-daemon-livekit` | the session room's delta stamping, and its five-entry `MultiRpcService` |
| `tddy-web` | 6 hooks, 9 components, 5 Cypress fakes; `session_agents_pb.ts` and `activity_pb.ts` generated, 78 exports leaving `connection_pb.ts`, 25 call sites re-pointed |

**`tddy-sandbox-app` was predicted and turned out not to be a consumer at all.** The changeset listed
"its service-name guard" as a migration item. Its guard matches
`connection.ConnectionService/ExecuteTool` — family A, which node 8 owns — and its daemon client
dials `ConnectionServiceClient` for methods that stayed. The package is untouched by this PR. That
is the mirror image of node 6's finding, where `tddy-sandbox-app` was the consumer **no** document
accounted for: the lesson both times is that the four-transport consumer list has to be measured, not
predicted.

## Baseline

Measured per touched package rather than as a whole-workspace figure, per this repo's scoped-
verification practice.

| Gate | Result |
|---|---|
| `cargo test -p tddy-session-agents` | **47 passed / 0 failed** (crate did not exist) |
| `cargo test -p tddy-session-activity` | **0 passed / 0 failed — the crate has no tests** (crate did not exist) |
| `tddy-service` | 105 lib + 16 split, 0 failed |
| `tddy-daemon --lib` | 277, 0 failed |
| `local_token_uds` | 13 (was 11), 0 failed |
| `session_hook_cli` | 9 (new), 0 failed |
| `tddy-sandbox-runner --lib` | 37 (was 35), 0 failed |
| `tddy-coder --lib` | 99 (was 98), 0 failed |
| `tddy-session-sync` | 67, 0 failed |
| `tddy-daemon-livekit` | 99, 0 failed |
| `tddy-web` Cypress | 30 affected specs, 228/228 |
| clippy `--all-targets -D warnings`, `cargo fmt` | clean across the eleven touched packages |

Not measured, and stated rather than claimed: `in_jail_conversation_acceptance` (see above), and the
LiveKit cross-host suites, which need Docker and are load-sensitive — run them individually and
re-run a failure in isolation before believing it.

## Package entries

- [`packages/tddy-session-agents/docs/changesets/`](../../../packages/tddy-session-agents/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-session-activity/docs/changesets/`](../../../packages/tddy-session-activity/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-daemon/docs/changesets/`](../../../packages/tddy-daemon/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-service/docs/changesets/`](../../../packages/tddy-service/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-sandbox-runner/docs/changesets/`](../../../packages/tddy-sandbox-runner/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-tools/docs/changesets/`](../../../packages/tddy-tools/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-coder/docs/changesets/`](../../../packages/tddy-coder/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-discovery/docs/changesets/`](../../../packages/tddy-discovery/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-session-sync/docs/changesets/`](../../../packages/tddy-session-sync/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-daemon-livekit/docs/changesets/`](../../../packages/tddy-daemon-livekit/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
- [`packages/tddy-web/docs/changesets/`](../../../packages/tddy-web/docs/changesets/2026-09-09-unbundle-session-agent-services.md)
