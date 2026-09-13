# 2026-09-11 — The terminal, context and session-file services

**Type:** Architecture

Node 6 of the `#unbundle` stack ([#475](https://github.com/uppin/tddy-coder/pull/475), 6 of 9, based
on node 5's branch, [#474](https://github.com/uppin/tddy-coder/pull/474)). Twenty-two RPCs leave
`connection.ConnectionService` — **72 → 50** — and `tddy-codegen`'s tonic adapter generator, a stub
until now, is implemented across all four method shapes.

PRD: [docs/ft/daemon/changelog/2026-09-11-unbundle-session-io-services.md](../../ft/daemon/changelog/2026-09-11-unbundle-session-io-services.md).

## The two coordinates

| Coordinate | Methods | Served by |
|---|---:|---|
| `terminal_session.TerminalSessionService` | 9 | [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md), and `tddy-coder`'s session participant for 7 of them |
| `session_files.SessionFilesService` | 13 | [`tddy-session-files`](../../../packages/tddy-session-files/docs/session-files-service.md), behind the daemon's peer-routing wrapper |

`terminal_session.proto` was not written here. It already existed, declared nine rpcs that duplicated
the terminal family exactly, and was **served nowhere** — a proto extracted without its served
coordinate, which is the failure mode this whole stack is shaped to avoid. What was genuinely shared
was the *bridge*, and its two call sites hand-converted between the two message sets. This node
serves the coordinate and deletes the converters.

The plan said there were two converters. There were **six**: two named in the daemon and four
anonymous inline ones in `tddy-coder/src/session_participant/mod.rs`, invisible to the name-based
grep the plan used. A CI sweep now greps both message names across both packages, so it enforces what
it claims; the earlier version looked for one needle under one directory and was satisfiable by
deleting a doc comment.

`connection.proto` needed **no** `reserved` field numbers. Nothing that stayed referenced a moved
message, and protobuf has no `reserved` for service methods — so a header note records the 22 vacated
coordinates instead. The plan's worry that the schema would carry the history of this split
permanently did not materialise.

## `types.proto` holds exactly one type

`HostDocumentScope`, because two separately served services reach it: `StartSession`, which stays on
`connection.ConnectionService`, names the staged attachments to materialise and each carries a scope;
and `ReadHostDocument`, which is now on `session_files.SessionFilesService`. A test pins the file at
one declaration, so anything added later has to clear the same bar.

## The generator

`generate_tonic_adapter` emits a complete tonic server-trait impl: unary, server-streaming (with the
associated `…Stream` type), client-streaming and bidirectional. Bodies call the shared
`tddy_service::to_tonic_status` rather than inlining a conversion, so a refusal cannot reach two
transports as two different gRPC codes. It gained a `tonic_trait_path` config field, which is what
lets two protos that already set `generate_tonic_adapter: true` without a tonic pass
(`echo_service.proto`, `token.proto`) keep their struct-and-`new()` shape and their public
re-exports. Reference: [tonic-adapter.md](../../../packages/tddy-codegen/docs/tonic-adapter.md).

A declarative macro cannot substitute: `#[tonic::async_trait]` rewrites the signatures of the trait it
is applied to, and a macro cannot see through that rewrite to generate the bodies.

**The cost argument that justified building it here was wrong; the capability argument was right.**
[`docs/dev/todo/2026-09-09-tonic-adapters-are-hand-written-per-service.md`](../todo/) predicted the
cost was "linear in the `#unbundle` stack: every remaining node that splits a service out of
`connection.ConnectionService` writes another one". Nodes 2, 3, 4 and 5 wrote **zero** — an adapter is
needed only for a service kept reachable on the daemon's local UDS socket, and their subsystems were
never on it. This node's plan then over-corrected and sized itself at **22** hand-written methods, by
assuming it would keep both its families UDS-reachable. It needed **9**. Nothing dials a session-file
method over that socket, checked across both in-jail binaries, so mounting `session_files` there would
have been surface with no caller — the same failure the node spent a milestone fixing in the other
direction. So the real figure was close to the ~9 the entry's own reasoning dismissed as not worth a
generator.

What carries the decision is the shape, not the count: `StreamSessionTerminalIO` is the **only**
bidirectional method in the whole 90-method surface. A generator built in any other node would have
handled two shapes and been found incomplete the first time it met a bidi method. And it is not
theoretical — the generated adapter is what `tddy-sandbox-app`, inside every jail, dials for its
terminal stream on the daemon's local socket.

The entry is **closed**. Two follow-ups replace it:
[the two remaining hand-written adapters](../todo/2026-09-11-node-ones-two-tonic-adapters-are-still-hand-written.md)
and [the coder's string dispatch](../todo/2026-09-11-the-coder-participant-dispatches-connection-rpcs-by-string.md).

## Two servers, and a parity test that makes the lockstep enforceable

`tddy-coder`'s session participant is the second server of the terminal family, and the repo has a
recorded incident of the two drifting — a session that "would have opened tail-first when reached
over HTTP and head-first when reached over LiveKit". So the participant moved to the new coordinate
in the same PR, and it registers `tddy-terminal-rpc`'s own entry constructor rather than writing
handlers: the replay model, the offsets and the ACK framing are one implementation.

It answers **seven** of the nine. `WatchTerminalControl` and `StreamSessionTerminalIO` are refused,
each for a reason rather than an omission, recorded in
[`docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md`](../todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md).
`packages/tddy-coder/tests/two_server_parity_acceptance.rs` opens one session through both servers
and asserts the seven answer identically.

Its dispatch is still a string match that discards the service name, which is why the participant
registers two separate `ServiceEntry`s rather than one service under two names — a single one would
serve `ListExecTools` on the terminal coordinate and `StreamTerminalOutput` on the connection one.

## The sandbox terminal branch was a second copy of the bridge

Four terminal methods also branched on the sandbox registry, and `StreamTerminalOutput`'s branch was
a hand-rolled reimplementation of the bridge's replay and offset arithmetic — its own comment said
*"matching `tddy_terminal_rpc::bridge`"*. Two implementations of one offset contract, in the surface
whose purpose is to be the single terminal surface.

It is unified through `TerminalSessionStore`: a `SandboxTerminalSession` adapter in `tddy-daemon`
beside the existing `DaemonTerminalSession`, and a composite store that resolves a sandbox session
first. All four branches are gone, along with 104 further lines nothing constructed once they went.
No predecessor crate was touched — `SandboxSessionState`'s `stdout_tx`, `capture` and `stdin_tx` were
already public, and the three answers a jail cannot give are synthesised to preserve behaviour.

**Three observable changes for sandboxed sessions, taken deliberately:**

| | Before | After |
|---|---|---|
| `GetTerminalHistory` | `not_found` | real offset-anchored chunks |
| `StreamSessionTerminalIO` | live only — no prologue, replay or anchoring frame | replays like every other terminal |
| `StreamTerminalOutput`, TAIL | the whole retained buffer in 32 KiB frames | prologue + last 8 KiB, scroll-up for the rest |

The third is the only regression-shaped one and is coherent only because of the first: the scrollback
is still reachable, paged rather than pushed on open, which is how every non-sandboxed terminal has
always behaved. Keeping a mode branch for sandbox sessions was rejected — it leaves a sandbox-specific
arm in the one surface supposed to have none.

`packages/tddy-daemon/tests/sandbox_terminal_parity_acceptance.rs` (10 tests) builds a real
`SandboxSessionState` and carries the deleted loop verbatim as its oracle.

⚠ **An honest gap in the end-to-end evidence.** The two daemon tests that would catch a
sandbox-terminal regression through the full stack — `sandboxed_claude_cli_terminal_io_round_trips`
and `sandboxed_session_streams_demo_tui_dimensions_in_terminal` — die on the pre-existing
`ConnectionServiceImpl::self_arc called before set_self_handle` harness fault *before reaching any
terminal RPC*, so they neither confirm nor deny this change. That is why the parity suite exists at
the store/adapter level.

One defaulted trait method was added to the bridge's `TerminalSession`: `resizable() -> bool`. It
gates the post-resize drain, which against a terminal with no PTY master would discard live bytes no
replay chunk covers — a data-loss bug this node's own sandbox unification would otherwise have
introduced. Node 5's `## Dependencies` row says this node does not change the bridge's trait shapes,
and this is a change to one; it is additive and defaulted, so no implementor outside the crate
changed, but a reviewer of node 5 should know the trait grew a method from above.

## The fourth transport

Removing the 22 rpcs broke the build on a consumer no document in the stack accounted for.
`tddy-sandbox-app` — the binary inside every jail — dials `connection.ConnectionService` over the
daemon's **local UDS socket** with tonic and opens the bidirectional terminal stream. Consumers are
spread across four transports, not three: Connect-HTTP (`tddy-web`), LiveKit (`tddy-coder`'s
participant), in-process `ServiceEntry` (the daemon), and that socket.

The symptom was a **compile error in `tddy-sandbox-app`**, not a failing test, so no amount of
test-suite green would have caught it. The socket now mounts a fourth tonic service and it is the
generated adapter.

**Process note for nodes 7, 8 and 9:** run `cargo build --workspace` once before any proto removal.
It is the documented exception to this repo's scope-it-locally rule — a removal can break any
package, and a two-minute local sweep beats a 25-minute CI round trip that only reveals the first
broken one. Eight of this node's milestones were verified locally and CI reported green on every one;
the ninth was pushed with its gates interrupted, and it is the one that broke the build.

## Four things the move silently lost, and how each surfaced

A 3,500-line relocation dropped four behaviours, and **none** was caught by the verification run for
the milestone that caused it. The pattern is specific: **a mechanical call-site substitution silently
changes which layer answers.** Both of the first two were `X.method(…)` → `X.something().method(…)`
rewrites that compiled and type-checked perfectly.

| Loss | How it surfaced | Fix |
|---|---|---|
| the in-jail UDS break | a compile error in another package | the socket mounts the terminal service |
| the routing fork sat on the **transport**, so every in-process caller was served locally whatever `daemon_instance_id` it named | 5 failing cross-host tests | routing moved onto the generated `SessionFilesService` trait |
| the context handlers dropped `tokio::time::timeout`, so a stalled read hung the RPC forever | uncovered — nothing tested it | the deadline restored as a port, 3 new tests |
| the split-context caller re-implemented the read when the handler moved, losing the deadline, the gate and the single path | uncovered | the duplicate deleted; it reads through the served surface, 1 new test |

Moving routing up also fixed an ordering the transport wrapper had inverted: the five staging methods
now authenticate **before** classifying, as `connection.ConnectionService` did. The wrapper
classified first, which let an unauthenticated request drive an outbound forward — the exact
inversion `stream_start_session_refuses_an_invalid_token_before_it_classifies_the_route` exists to
forbid. That test did not catch it because it covers `StartSession`, which never moved.

**Two tests were passing for the wrong reason, which is worse than the five that failed.**
`stream_read_host_document_forwards_to_the_peer_that_owns_the_document` staged onto what it believed
was the peer, read back from the same host, and asserted success **without a byte crossing**. A
cross-host suite can pass while proving nothing, so "the suite is green" is not evidence that
forwarding works — only an assertion on the *peer's* state is.

**Deadline tests need a deterministic stall, not a sleep.** Both new suites use a current-thread
runtime with `max_blocking_threads(1)`, that one thread occupied by a read parked on a channel until
the assertion is made. The obvious seam — a `ContextSource` double — does not work, because that
trait belongs to the syncer and no handler goes through it; nor does a FIFO, since both readers gate
on `is_file()` and refuse one rather than blocking.

## Boundaries held, and one that moved

- `cli_session_manager.rs` stays in the daemon. It is PTY session *lifecycle* and the origin of the
  `TaskRegistry` several services share; the terminal service reaches it through
  `TerminalSessionStore`.
- `daemon_instance_id` peer routing stays in the daemon. Eight of the thirteen session-file methods
  route, and that needs the eligible-daemon roster, the common room slot and the per-method LiveKit
  clients — a session-file reader that reached for them would be back inside the module it was
  extracted from.
- `to_tonic_status` **moved** from `tddy-daemon` to `tddy-service`, because generated adapters land in
  `tddy-service`'s and `tddy-terminal-rpc`'s `OUT_DIR` and neither depends on `tddy-daemon`.
  (`tddy-rpc` has its own conversion, but it pins tonic 0.11 against everything else's 0.12, which is
  why a hand-written one exists at all.) Re-pointing the three adapters' imports is a one-line edit
  each.
- `paired_agent` moved to `tddy-core`, beside the `SessionMetadata` it reads, to cut the
  `context_files` ↔ `context_sync` ↔ `split_session` cycle. It is a pure accessor over two trimmed
  optional fields with two real callers, so the cut is small and it inverts the edge the right way:
  `split_session` depends on the new crate, and all ten modules move.
- The sandbox hop got a **better** answer than the plan's. Re-pointing the sandbox pass's
  `.connection.SessionTerminalOutput` extern path at `terminal_session`'s message is impossible —
  `tddy-terminal-rpc` depends on `tddy-service`, so naming its message from `sandbox.proto` is a
  dependency cycle. `sandbox.proto` instead owns a `SandboxTerminalOutput` carrying only what crosses
  that hop. The four fields it drops are capture-ring and input-ack metadata the jail never set and
  the host relay never read; they are `reserved`, and the three survivors keep their numbers, so the
  bytes on that hop are unchanged.

## What did not move, and the measurement behind each

`## Affected Packages` promised `tddy-terminal-rpc` would gain "the 3 PTY modules". It gained **one
function**.

- **`pty_registry.rs` was deleted, not moved** — six lines of `pub use tddy_pty::{…}` with two
  importers, who now name `tddy_pty` directly. Relocating a re-export shim moves nothing.
- **`pty_runtime.rs` stays.** Its `crate::` paths are shims, so the plan was right that there is no
  *daemon* coupling — but the crates behind them are not free. `privilege_drop` and
  `spawn_path_extra_for_home` come from `tddy-daemon-kernel`, which depends **non-optionally** on
  `tddy-livekit`. `tddy-tools` depends on `tddy-terminal-rpc` non-optionally and carries a `livekit`
  feature whose entire purpose is to keep the LiveKit/webrtc SDK out of an in-jail build that only
  speaks gRPC. Measured: `cargo tree -p tddy-tools --no-default-features -e normal | grep -c livekit`
  is **0** today, and moving `pty_runtime` would make it non-zero unconditionally.
- **What did move is `login_shell_for_os_user`** → `tddy-terminal-rpc/src/login_shell.rs`, a pure
  passwd lookup with no daemon state, because `StartTerminalSession` is served from that crate.
  `connection.ConnectionService` was re-pointed at the same function while both were mounted, so the
  two coordinates could not start a session in different shells. Its last-resort shell is now the
  named `DEFAULT_LOGIN_SHELL` rather than an inline `"/bin/bash"` — a host without it (a minimal Nix
  closure, Alpine) fails the spawn, and the failure should name the assumption.
- **`terminal_session_adapter.rs` stays**, as `## Boundaries` implies: it binds the daemon's own
  managers to the trait, which is what the trait is for, and it is where the sandbox adapter went.

## The test-suite split: 5 of 14 moved

Five suites moved with assertions untouched — the whole content diff is import paths:
`context_file_frames_unit.rs`, `context_files_acceptance.rs`,
`pr_stack_child_doc_attachment_acceptance.rs`, `pr_stack_context_docs_acceptance.rs`,
`staged_attachment_path_validation.rs`.

Nine could not, each pinned by a symbol that stays in `tddy-daemon` — `ConnectionServiceImpl`,
`test_util::{test_service, TEST_TOKEN}`, `split_session::build_split_context_dir`,
`multi_host::EligibleDaemonSource`, `runtime::spawn_common_room_discovery_task`. Moving any of them
would put `tddy-daemon` back on `tddy-session-files`' dependency path and defeat the extraction — the
same measurement node 4 made (5 of 18 there). All nine pass where they are.

## Files over 500 lines: 10, and the largest is in the daemon

The plan's file-budget count said 8 and named the wrong file as the growth target. Measured:

| File | Lines |
|---|---:|
| `tddy-session-files/src/host_documents.rs` | **819** |
| `tddy-daemon/src/connection_service/svc_split_context_from_codebase_host.rs` | **803** |
| `tddy-session-files/src/session_context_docs.rs` | 639 |
| `tddy-session-files/src/context_sync.rs` | 634 |
| `tddy-session-files/src/session_attachments.rs` | 613 |
| `tddy-session-files/src/service.rs` | 553 |
| `tddy-terminal-rpc/src/service.rs` | 548 |
| `tddy-session-files/src/lib.rs` | 539 |
| `tddy-session-files/src/stack_doc_attachments.rs` | 519 |
| `tddy-session-files/src/context_files.rs` | 509 |

`svc_split_context_from_codebase_host.rs` grew from 314 and is the largest single-file growth in the
PR — it was absent from the plan's list entirely, and it is in the **daemon**, not the new crate.
`host_documents.rs` grew by the framing function it absorbed. `lib.rs` is roughly 440 lines of test
module, so listing it beside the others overstates it by an order of magnitude.

None was split. A split for line count alone cuts cohesive units and puts churn on top of a rename,
and splitting a file nodes 7-9 also touch cascades conflicts through their diffs.

## Dead surface this node introduced, removed

A readiness pass over the diff found seven items with no production consumer; all were removed, each
preceded by a workspace grep. Notably: `TERMINAL_OUTPUT_FRAME_MAX_BYTES`, `chunk_terminal_output` and
`sandbox_replay_frames` (kept compiling only by `#[cfg_attr(not(test), allow(dead_code))]` and
referenced only by the two unit suites written for them, whose subject is now the parity suite);
`build_session_files_entry`, whose only caller was the crate's own test helper and which therefore
proved registration of an *unrouted* lookalike; `into_tonic_stream` / `history_into_tonic_stream`,
whose last plausible consumer was deleted here.

`DEFAULT_INITIAL_FRAME_BYTES` (8 KiB) carries the magnitude guard the deleted
`TERMINAL_OUTPUT_FRAME_MAX_BYTES` had — a compile-time assertion that the default is nonzero and
under a megabyte, checked at compile time because a default nobody passes has no test that would
notice.

## Baseline

Measured with `--no-fail-fast` on both sides over 135 binaries: **1318 passed / 37 failed / 3
ignored** with this node, and the failure set is **byte-identical** to the base's 37, in three
pre-existing families — the documented sandbox `self_arc` group, `Once instance has previously been
poisoned` where no LiveKit testkit is running, and the sqlx model-registry store. No regressions.

The originally recorded "1027 passed / 1 failed" was a **fail-fast** number: `cargo test` stops at
the first failing binary, so it was never comparable to a whole-suite figure, and a later node reading
it as one would mis-measure its own regression.

| Gate | Result |
|---|---|
| `cargo test -p tddy-session-files` | **155 passed / 0 failed** (crate did not exist) |
| `cargo test -p tddy-codegen` | **5 passed** — and these five were `cfg`'d out of every run until this node removed the gate |
| `scripts/generated-code.sh check` | clean; two files that had never been committed are now in |
| CI | `Rust lint` · `Rust build` · `Rust build (arm64)` · `Generated code` · `Web tests` 2630/2630 · `Rust tests` 6560/6562 |

CI settles the local noise: the 37 failures are environmental, and CI's proper environment passes all
of them.

## Environment notes worth carrying

- The shared `tddy-livekit-testkit` container can be **unusable while appearing healthy**: its
  LiveKit advertises `127.0.0.1:7881`/`7882` for ICE while Docker maps those elsewhere, so every
  participant fails at connect with `wait_pc_connection timed out` — 8 tests "failing" in 542 s with
  no assertion reached. Unsetting `LIVEKIT_TESTKIT_WS_URL` lets testcontainers start a correctly
  mapped one and the suites run in 20-90 s.
- The cross-host suites start two daemons against one LiveKit container and are genuinely
  load-sensitive. Run them individually and re-run a failure in isolation before believing it.

## The base moved twice during one green phase

Node 5 force-pushed a rewritten history while milestone 1 was building, so the branch went stale
between the step-0 rebase and the first milestone push. Both rebases used
`git rebase --onto <new base> <recorded pre-rebase tip>`, which is what keeps a predecessor's old
commits from being replayed as this node's own; a plain `git rebase` at that moment would have
duplicated node 5's entire delta into this PR's diff. Every milestone re-read the base tip
immediately before pushing rather than trusting the tip step 0 saw.

Two predecessor reds arrived on this branch and **both resolved themselves mid-flight** — node 4's
`connection_service_no_longer_declares_the_rooms_stream` and node 5's
`session_tool_client::tests::refuses_a_call_on_a_session_with_no_transport`. That is the argument for
re-verifying inherited failures after every rebase rather than carrying the first reading forward.

## Package entries

- [`packages/tddy-daemon/docs/changesets/`](../../../packages/tddy-daemon/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-session-files/docs/changesets/`](../../../packages/tddy-session-files/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-terminal-rpc/docs/changesets/`](../../../packages/tddy-terminal-rpc/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-codegen/docs/changesets/`](../../../packages/tddy-codegen/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-coder/docs/changesets/`](../../../packages/tddy-coder/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-service/docs/changesets/`](../../../packages/tddy-service/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-web/docs/changesets/`](../../../packages/tddy-web/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-task/docs/changesets/`](../../../packages/tddy-task/docs/changesets/2026-09-11-unbundle-session-io-services.md)
- [`packages/tddy-core/docs/changesets/`](../../../packages/tddy-core/docs/changesets/2026-09-11-unbundle-session-io-services.md)
