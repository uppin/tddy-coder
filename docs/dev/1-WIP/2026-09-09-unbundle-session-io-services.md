# Changeset: the terminal, context and session-file services

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **6 of 8**. PR [#475](https://github.com/uppin/tddy-coder/pull/475).
Base: `feature/unbundle/tools-thinning` (node 5, PR #474)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-session-io-services-initial-discovery.md](./2026-09-09-unbundle-session-io-services-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

22 methods leave `connection.ConnectionService`, taking 3,842 prod LoC of source with them.

| New coordinate | Families | Methods | Served by |
|---|---|---:|---|
| `terminal_session.TerminalSessionService` — **exists already, served nowhere** | K | 9 | `tddy-terminal-rpc` |
| `session_files.SessionFilesService` — new | I, J, R, S | 13 | `tddy-session-files` *(new crate)* |

**Family K finishes an extraction the repo already paid for.**
`packages/tddy-terminal-rpc/proto/terminal_session.proto` declares 9 rpcs that duplicate family K
exactly. `grep -rn 'TerminalSessionService\|terminal_session\.'` outside that package returns **zero
hits**. What is shared is the *bridge* — `serve_stream_terminal_output_with`,
`serve_get_terminal_history_with`, `serve_stream_session_terminal_io_with` and the
`TerminalSession`/`TerminalSessionStore` traits — and its two call sites **hand-convert**
`connection.SessionTerminalInput` → `terminal_session.SessionTerminalInput` at
`connection_service.rs:439-457`. This node serves the coordinate and **deletes both converters**.

**It also implements `tddy-codegen`'s `generate_tonic_adapter`.** That is a second deliverable in one
node, and the justification is arithmetic plus a capability nobody else can supply:

- **Cost.** `generate_tonic_adapter` is a stub, so every service kept reachable on the local UDS
  socket needs an adapter written by hand — a literal `async fn` per method unwrapping a
  `tonic::Request`, calling the Connect-RPC impl, and mapping the result back. Node 1 wrote two
  (17 methods). Following node 1's precedent of preserving UDS reachability, node 6 would write
  **22** — `session_files.SessionFilesService` (13) plus `terminal_session.TerminalSessionService`
  (9). More than node 1, and the most of any node.
- **Capability.** `StreamSessionTerminalIO` is the **only bidirectional method in the entire
  90-method surface** — 69 unary, 20 server-streaming, 1 bidi, and the bidi one is family K's. A
  generator built in any other node would handle unary and server-streaming and be discovered
  incomplete the first time it met a bidi method. Node 6 is the only node that forces it to be
  complete.

A macro cannot substitute: `#[tonic::async_trait]` rewrites the signatures of the trait it is applied
to, and a declarative macro cannot see through that rewrite to generate the bodies. It is a codegen
job or it is hand-written.

Source that moves: the context/documents subsystem (`host_documents.rs`, `context_sync.rs`,
`session_context_docs.rs`, `context_files.rs`, `stack_doc_attachments.rs`,
`session_workflow_files.rs` — 2,197 prod LoC), the attachment and upload modules
(`session_attachments.rs`, `session_attachment_staging.rs`, `session_file_upload.rs`,
`session_uploads.rs` — 1,200), and the PTY modules (`pty_runtime.rs`, `terminal_session_adapter.rs`,
`pty_registry.rs` — 445).

## Boundaries

This PR explicitly does **not**:

- Move `cli_session_manager.rs`. It is the PTY *session lifecycle*, and it is where the `TaskRegistry`
  five services share actually originates (`connection_service.rs:2049`). Family C keeps it, and this
  node's terminal service reaches it through the `TerminalSessionStore` trait rather than owning it.
- Take families B, M or N (node 7) or A, L or P (node 8).
- Retire `connection.proto`'s terminal messages' *history*. Field numbers vacated by the 22 moved
  methods' messages are marked `reserved`, following the precedent at `connection.proto:1031`.
- Rename any message while moving it. `terminal_session.proto`'s versions become canonical because
  they already exist, not because they are better; a rename would put a second breaking change in the
  same web migration.
- Change the terminal control mutex's semantics. `ClaimTerminalControl` / `WatchTerminalControl` move
  verbatim.
- **Retro-fit the generator over node 1's two hand-written adapters, or the connection adapter.**
  The generator is used for the services *this* node adds. Regenerating `host_tonic_adapter.rs` and
  `worktree_tonic_adapter.rs` would put a rewrite of a predecessor's files in this node's diff, and
  `connection_tonic_adapter.rs` is deleted by node 9 anyway. Replacing the two survivors is a
  follow-up whose whole diff is a deletion, recorded in `docs/dev/todo/`.
- **Generate anything but the tonic adapter.** `generate_rpc_server` and the trait emission are
  untouched; this node fills in one stub.
- Force every file under 500 lines. `host_documents.rs` (595), `context_sync.rs` (595),
  `session_context_docs.rs` (558), `session_attachments.rs` (570) and `worktree_files.rs`-adjacent
  seams are over budget; split where cohesive, and whatever stays over is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate` | every move here is a plan the operation executes | add, extend or fix the operation |
| `n1` host-worktree-services | `packages/tddy-service/proto/types.proto` and the established pattern for splitting a service out of `connection.proto` | `session_files.proto` imports `types.proto` for `SessionEntry`, `SessionContextDoc`, `SessionAttachment`, `StagedAttachmentRef` and `HostDocumentRef` rather than duplicating them | change `types.proto`'s shape; a message it lacks is requested upward |
| `n1` host-worktree-services | `tddy-daemon-kernel` exporting `HOST_DOCUMENT_FRAME_BYTES` | `context_files.rs:41` imports it, and it is the only thing the context subsystem reaches into `connection_service` for | re-export or re-define the constant |
| **`n5` tools-thinning** | **`pty_relay` and the terminal bridge, now in `tddy-terminal-rpc`** | this node serves `terminal_session.TerminalSessionService` from that crate, on top of the bridge and the `pty_relay` client node 5 put there. Starting before node 5's contract is pushed means building against a surface about to move | move `pty_relay` again, or change the bridge's trait shapes |
| `n2`, `n3`, `n4` | nothing this PR consumes | — | — |

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-service/proto/session_files.proto` declaring all 13 methods of families I, J, R and
  S, importing `types.proto`.
- The `terminal_session.TerminalSessionService` server wiring in `tddy-terminal-rpc` — the
  `build_terminal_session_entry(...) -> ServiceEntry` signature, body annotated
  `// TODO(session-io-services): implement`.
- `packages/tddy-session-files/src/lib.rs` declaring the `ContextSource` trait port and the crate's
  entry constructor with real signatures.
- The `Cargo.toml`s and workspace `members` entry.
- The failing acceptance tests, including the two-server parity test below.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 3 of 3
**Greenable independently:** **not until node 5 is green.** The terminal service is served from
`tddy-terminal-rpc`, and node 5 is what puts `pty_relay` and the bridge's consumers there.
**Concurrent with:** nodes 4, 7 and 8 — disjoint proto families and disjoint source modules. Node 7's
family M and this node's family K both touch `tddy-coder/src/session_participant/mod.rs`, so **expect
a conflict there on every cascade** and resolve by keeping both dispatch changes.
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8

⚠ **Two recurring conflicts**: `packages/tddy-daemon/src/runtime.rs` (every node) and
`packages/tddy-coder/src/session_participant/mod.rs` (nodes 6, 7 and 8).

## Affected Packages

- **tddy-codegen** — `generate_tonic_adapter` implemented: unary, server-streaming and bidirectional
- **tddy-session-files** *(new)* — the 10 context, attachment and upload modules
- **tddy-terminal-rpc** — serves `terminal_session.TerminalSessionService`; gains the 3 PTY modules
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 13 modules and 3,842 prod LoC leave;
  both hand converters deleted
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — loses 22 endpoint entries
- **tddy-coder**: `src/session_participant/mod.rs` — its family-K dispatch moves **in lockstep**
- **tddy-service**: `session_files.proto` appears; `connection.proto` loses 22 rpcs; the
  `.connection.SessionTerminalOutput` extern path in the sandbox tonic pass re-points
- **tddy-web**: 11 hooks and components, plus the Cypress terminal and file fakes
- **tddy-tools**: `pty_relay` (in `tddy-terminal-rpc` since node 5) names two moved coordinates

## Related Feature Documentation

- [PRD-2026-09-09-session-io-services.md](../../ft/daemon/1-WIP/PRD-2026-09-09-session-io-services.md)

## Summary

The terminal family moves to the service that was written for it and never served, deleting two
hand-written converters; context sync, workflow files, uploads and staged attachments become
`session_files.SessionFilesService` in a new `tddy-session-files` crate.

## Background

See the PRD. Two facts drive this node:

1. **`terminal_session.proto` is the cautionary precedent for this whole stack.** The repo extracted
   a proto without moving the served coordinate and got a duplicate schema, two hand converters and no
   caller. That is the failure mode every node in `#unbundle` is designed to avoid, and this node is
   where it gets repaired.
2. **`tddy-coder` is a second server for family K.** Its session participant string-dispatches
   `("connection.ConnectionService", method)` and returns `Unimplemented` for the rest. The repo has
   already been bitten by drift between the two — *"the same session would have opened tail-first when
   reached over HTTP and head-first when reached over LiveKit"* — so the lockstep move is a correctness
   requirement, not tidiness.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `2026-07-28-terminal-lazy-scroll-up-livekit-transport-unified-surface.md` and `2026-07-28-terminal-native-scrolling-model.md`

Both describe the terminal transport surface as needing unification, and this node is the moment it
either happens or is locked in. Every route around it is wrong: serving family K from
`terminal_session.TerminalSessionService` while leaving the two hand converters in place would mean
three message shapes for one stream (`connection.*`, `terminal_session.*`, and the converter between
them) in a service whose entire purpose is to be the one terminal surface. The converters are deleted
as part of the move. Earns a `## Scope` line.

### ⛔ BLOCKING — the `tddy-coder` lockstep requirement

Not a `docs/dev/todo/` entry but a documented incident, recorded in
`packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`. `tddy-coder`'s
session participant must move to the new terminal coordinate in this same PR. Splitting it into a
follow-up would leave a window in which HTTP and LiveKit answer the same session differently. Earns a
`## Scope` line.

### ⚠ DURING — `2026-08-14-a-claude-cli-split-agent-has-no-route-to-its-own-attachments.md`

A split agent has no route to its own attachments — and the attachment modules move here. The move
must not make the missing route harder to add: `session_attachment_staging.rs` keeps its scope
parameter rather than being simplified on the way out. Recorded, not fixed here.

### ⚠ DURING — `2026-08-29-agent-context-sync-the-re-sync-trigger-is-not-wired.md` and `2026-08-29-agent-context-sync-tddy-coder-src-remote-rs-holds-a-contradictory-dead.md`

Both are in `context_sync.rs`'s path. They move unchanged; recorded so a reviewer seeing them in
`tddy-session-files` knows they are inherited.

### ℹ ANSWERED — `2026-08-14-tddy-rust-typescript-tests-gen-is-badly-stale-and-nothing-detects-it.md`

Node 1 added the drift gate. This node is the largest single regeneration in the stack (22 methods),
so it is the first real exercise of that gate. Confirm at wrap that the gate caught what it should.

## Scope

- [x] **Proto**: `session_files.proto` (13 methods) ✅; `connection.proto` loses 22 rpcs ✅ (72 → **50**). **No field number needed `reserved`** — nothing that stayed referenced a moved message, and protobuf has no `reserved` for service methods, so a header note records the 22 vacated coordinates instead (`connection.proto:11-18`)
- [x] **Tonic adapter generator**: `tddy-codegen`'s `generate_tonic_adapter` implemented — unary,
      server-streaming (with the associated `…Stream` type) and **bidirectional**; emits calls to the
      shared status conversion rather than inlining its own
- [~] **Generated adapters used**: the **terminal** service reaches the UDS socket through the generated adapter ✅ — and that is what the jail dials. **`session_files` is deliberately not on that socket**: nothing dials a session-file method over UDS (checked across both in-jail binaries), so mounting it would add surface with no caller. This halves the premise that sized this node at 22 hand-written methods — see `## Green-phase corrections`
- [x] **⛔ Terminal surface unified**: `terminal_session.TerminalSessionService` served ✅; **all six converters deleted** — the plan said two, and the four inline ones in `tddy-coder` were invisible to a name-based grep
- [x] **`tddy-session-files`**: crate, 10 modules, its suites ✅ (155 tests)
- [~] **`tddy-terminal-rpc`**: serves family K ✅. **1 of the 3 PTY modules**, not 3: `pty_registry.rs` was a 6-line re-export and was deleted rather than relocated, and `pty_runtime.rs` had to stay — its shims resolve to `tddy-daemon-kernel`, which pulls LiveKit non-optionally into `tddy-tools`' in-jail build. Only `login_shell_for_os_user` moved
- [x] **⛔ `tddy-coder` lockstep**: its session participant moves to the new terminal coordinate in this PR
- [x] **Sandbox extern path** ✅, solved better than planned: rather than re-pointing the remap, `sandbox.proto` now owns a `SandboxTerminalOutput` carrying only what crosses that hop. Naming `terminal_session`'s message there would be a dependency cycle (`tddy-terminal-rpc` depends on `tddy-service`). The three surviving fields keep their numbers and the four dropped ones are `reserved`, so the wire is unchanged
- [x] **Web**: migrated ✅ (more than the 11 planned) and the fakes split per service, following node 1's `hostServiceBackend.ts` shape. Cross-package proto generation cost **one manifest line**
- [x] **File budget** recorded ✅ — 8 files over 500 lines, none split; splitting a file nodes 7-9 also touch would cascade conflicts through their diffs
- [x] **Baseline** ✅ — recorded per package, measured `--no-fail-fast` on both sides; the failure set is byte-identical to the base's
- [x] **Code Quality** ✅ — scoped clippy clean, `cargo fmt` clean, and CI's workspace-wide `Rust lint` passes
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

`connection.ConnectionService` has 72 methods after nodes 1 and 4. Families I (2), J (3), K (9),
R (3) and S (5) are among them. `terminal_session.proto` declares a byte-for-byte duplicate of family
K and is served nowhere; `connection_service.rs:439-457` converts between the two message sets.
`tddy-coder`'s session participant serves 14 methods across families K, L, M and N by string dispatch.
The sandbox tonic pass `extern_path`s `.connection.SessionTerminalOutput`.

### State B

`connection.ConnectionService` has **50 methods**. `terminal_session.TerminalSessionService` is served
by `tddy-terminal-rpc` on all transports and by `tddy-coder`'s participant; there is one terminal
message set and no converter. `session_files.SessionFilesService` is served by `tddy-session-files`.

### Delta

#### tddy-service
- **Proto**: `session_files.proto` added importing `types.proto`; `connection.proto` loses 22 rpcs and
  the messages that move, with vacated field numbers `reserved`; `terminal_session.proto`'s messages
  become canonical
- **Build**: one prost + one tonic pass for `session_files`; **the sandbox pass's
  `.connection.SessionTerminalOutput` extern path re-points to `terminal_session`**

#### tddy-daemon
- **Architecture**: 13 modules leave; both converters deleted
- **Implementation**: two `ServiceEntry` groups move behind the new crates' constructors

#### tddy-terminal-rpc
- **API**: `build_terminal_session_entry`; gains the 3 PTY modules; the bridge's traits unchanged

#### tddy-coder
- **Implementation**: `session_participant/mod.rs`'s family-K dispatch targets the new coordinate

#### tddy-web
- **Implementation**: 11 hooks and components; the terminal and file Cypress fakes split per service

## Implementation Milestones

- [x] M1 — `session_files.proto` generates; `types.proto` imported rather than duplicated ✅
- [x] M2 — `tddy-session-files` extracted; its suites pass ✅ (155 tests)
- [x] M3 — `terminal_session.TerminalSessionService` served from `tddy-terminal-rpc` ✅; PTY modules: **1 of 3 moved**, see above
- [x] M4 — all **six** converters deleted; the absence sweep now covers both message names across `tddy-daemon` **and** `tddy-coder`, so it enforces what it claims
- [x] M5 — participant moved; parity asserted at the wire over the 7 methods both servers serve (`two_server_parity_acceptance.rs`, 12 tests)
- [x] M6 — sandbox hop given its own message; `cargo build --workspace` clean
- [x] M7 — web migrated; ~300 component tests green (run per spec, not the 50-minute suite); baselines and file budget recorded

## Testing Plan

**Primary test level: integration, per package**, with one exception that is the most important test in
this node.

The moved suites carry most of the proof: `context_files_acceptance.rs` (644),
`context_sync_acceptance.rs` (626), `terminal_session_acceptance.rs`, the staging suites and the
upload suites all move with the code and are not rewritten.

**The new test is a two-server parity test.** Family K is served by both `tddy-terminal-rpc` (for the
daemon) and `tddy-coder`'s session participant. The repo's recorded incident is precisely that these
two drifted, so the test opens the *same* session over both the HTTP and the LiveKit participant and
asserts the two answers are identical — history offsets, stream mode, and control-claim outcome. That
assertion is what makes the lockstep requirement enforceable rather than aspirational.

Three further proofs:

- **`restructure verify --against <pre-move ref>`** from the repo root.
- **The moved-line diff**, alongside the visibility table.
- **A converter-absence grep**, asserted in CI rather than by eye: no source file references
  `connection.SessionTerminalInput` or `connection.SessionTerminalOutput` after this node.

`tddy-web` keeps `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

### tddy-terminal-rpc
- [x] **Integration**: all 9 methods answer at the registered coordinate (`terminal_session_service_acceptance.rs`, 14 tests)
- [x] **Integration**: `StreamSessionTerminalIO` carries a bidi session end to end (`terminal_session_bidi_acceptance.rs`, 9 tests)
- [x] **Integration**: `GetTerminalHistory` frames/offsets match the old coordinate (`terminal_history_parity_acceptance.rs`, 6 tests), plus `sandbox_terminal_parity_acceptance.rs` (10 tests) carrying the deleted loop verbatim as its oracle

### tddy-terminal-rpc + tddy-coder
- [x] **Integration**: the same session answers identically through the daemon's server and through
      `tddy-coder`'s session participant — history offsets, stream mode, control claim (`two_server_parity_acceptance.rs`)

### tddy-codegen
- [x] **Unit**: a unary method generates a delegating `async fn` (`generator.rs`)
- [x] **Unit**: a server-streaming method generates its associated `…Stream` type (`generator.rs`)
- [x] **Unit**: **a bidirectional method generates a `Streaming` request and a stream response** (`generator.rs`) — and these five tests were `cfg`'d out of every run until this node removed the gate
- [x] **Unit**: the generated body calls the shared status conversion (`generator.rs`)
- [ ] **DEFERRED — `generated_adapter_parity_acceptance.rs` was not written.** `## Boundaries` forbids regenerating node 1's two hand-written adapters, so there is no hand-written adapter this node replaces to compare against. The generated adapter's correctness is instead evidenced by it serving the jail's live terminal traffic on the UDS socket (`local_token_uds.rs`), which fails with `Unimplemented` if the mount is removed. Carried into the follow-up that replaces node 1's two adapters

### tddy-session-files
- [x] **Integration**: the registered coordinate serves and routes (`session_files_service_acceptance.rs`) — including that a request naming another daemon forwards rather than being served locally, which fails with a local `host_path` if the routing wrapper is bypassed
- [x] **Integration**: `context_sync_acceptance.rs` (18 tests) — stayed in `tddy-daemon`, pinned by `split_session::build_split_context_dir`
- [x] **Integration**: `staging_rpc_acceptance.rs` (7 tests), plus the 8 other pinned suites that stayed
- [x] **Integration**: the framing is covered, though **not in a file of that name** — it lives in `tddy-session-files`' own suite, where it writes a 2.5-frame document and asserts `[48 KiB, 48 KiB, 24 KiB]`. The red phase's version compared the kernel constant to itself and would have passed on an empty crate

### tddy-service
- [x] **Unit**: the converter-absence sweep passes — **not in a file of that name**, but as `no_source_converts_between_the_two_terminal_message_sets` in `unbundle_service_split.rs`, where the rest of this node's proto assertions already live. Widened from one needle under one directory (satisfiable by deleting a doc comment) to both message names across `tddy-daemon` and `tddy-coder`
- [x] **Unit**: the sandbox pass compiles (build-level) ✅ — via its own `SandboxTerminalOutput` rather than a re-pointed extern path

### tddy-web
- [x] **Cypress component**: the terminal specs pass at the new coordinate (the four `GrpcSessionTerminal*` specs, `TerminalInputAckAcceptance`, and others)
- [x] **Cypress component**: `SessionFilesTabAcceptance` (6) and `SessionInspectorFilesTab` (2) pass against the split `sessionFilesServiceBackend.ts`

### tddy-daemon
- [x] **Integration**: `connection.ConnectionService` declares **50** and none of the 22 ✅ — exactly the number `## Technical Changes` § State B predicted

## Decisions & Trade-offs

- **The generator lands here rather than on its own schedule, and I argued against that first.**
  Sized as "3-5 days of codegen to avoid ~9 hand-written methods", it was a clear loss — that
  estimate counted only the methods a *production* tonic client dials (`start_session` and
  `mint_local_token`, both node 9's). It ignored the precedent node 1 set: families that were on
  `ConnectionService` were reachable over UDS, and node 1 kept its two families reachable rather than
  silently dropping that. Node 6 following the same precedent is 22 hand-written methods, not 9. The
  arithmetic flips, and the bidi requirement means this node is also the only one that can produce a
  *complete* generator.
- **`to_tonic_status` stays shared, and the generator emits calls to it.** Three hand-written
  adapters already share it so they cannot drift on how a refusal maps to a tonic code. A generator
  that inlined its own conversion would reintroduce exactly that drift, between generated and
  hand-written adapters.
- **`terminal_session.proto`'s messages become canonical, not `connection.proto`'s.** They already
  exist and already have a `build.rs` pass; adopting them costs one extern-path re-point, while the
  reverse would mean deleting a proto the repo deliberately wrote. This is not a judgement that they
  are better — no message is renamed or reshaped, precisely so that this node carries one breaking
  change rather than two.
- **The two hand converters are deleted, not kept as an adapter.** Keeping them would leave three
  message shapes for one stream inside the service whose purpose is to be the single terminal surface,
  and this repo's rule is that fallbacks make a system unsafe. Deleting them is what makes the
  extraction real rather than nominal.
- **`cli_session_manager.rs` stays in the daemon.** It is the origin of the `TaskRegistry` that five
  services share, and it is session *lifecycle* rather than terminal I/O. The terminal service reaches
  it through the `TerminalSessionStore` trait, which is what that trait was written for.
- **Four families in one service.** I, J, R and S are context, workflow files, uploads and
  attachments — arguably four services. They are one because they are all *session file I/O*, they
  share `SessionAttachment`, `StagedAttachmentRef` and `HostDocumentRef`, and four services of two to
  five methods each would multiply the hand-written tonic adapters (one per gRPC-served service,
  because the codegen's adapter generator is a stub) for no reviewer benefit.
- **Families C and D are deliberately not touched**, even though `GetWorktreeSnapshot` and
  `ListProjects` are adjacent to file I/O. The endpoint of this stack is a daemon that owns session
  lifecycle and projects; taking those would leave nothing coherent behind.

## Technical Debt & Production Readiness

- [ ] Node 1's `host_tonic_adapter.rs` and `worktree_tonic_adapter.rs` remain hand-written; replacing
      them with generated ones is a follow-up whose whole diff is a deletion
- [ ] The split-agent attachment route (`2026-08-14-…`) is still missing; the move preserves the scope
      parameter it will need
- [ ] `tddy-coder`'s session participant remains a string dispatch rather than a generated trait impl;
      making it a real implementation is a `docs/dev/todo/` entry at wrap
- [x] **Not debt after all**: no field number in `connection.proto` was vacated. Nothing that
      stayed referenced a moved message, so there was nothing to `reserve`, and protobuf has no
      `reserved` for service methods — a header note records the 22 departed coordinates instead.
      The plan's worry that the schema would "carry the history of this split permanently" does not
      materialise; the only `reserved` this node adds is on `sandbox.proto`'s own new message, for
      the four fields that hop never carried.

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed** (a *fail-fast* count — see below) | **37 failures, byte-identical set to the base**, measured `--no-fail-fast` over 135 binaries; all three families environmental |
| `cargo clippy -p tddy-session-files -p tddy-service --all-targets -- -D warnings` | ✅ exit 0 | ✅ exit 0, and CI's workspace-wide `Rust lint` passes |
| `cargo test -p tddy-session-files` | crate did not exist | ✅ **155 passed / 0 failed** |
| `cargo test -p tddy-codegen` | 0 tests (the 5 were `cfg`'d out of every run) | ✅ **5 passed**, now reachable from a plain `cargo test` |
| `scripts/generated-code.sh check` | 2 files never committed (drift) | ✅ all four gated packages up to date |
| CI on `54ac24ef` | — | `Rust lint` ✅ · `Rust build` ✅ · `Rust build (arm64)` ✅ · `Generated code` ✅ · `Web tests` ✅ **2630/2630** · `Rust tests` **6560/6562** |

**The "Before" row was misleading, not merely stale.** `1027 passed / 1 failed` is a fail-fast
number — `cargo test` stops at the first failing binary — so it was never comparable to a
whole-suite figure, and any later node reading it as one would mis-measure its own regression. The
`## Green-phase corrections` section records the both-sides measurement that replaces it.

**The two tests still red are this node's own definition of done**, and nothing else in the
workspace is: CI's only failures on `54ac24ef` are
`connection_service_no_longer_declares_the_session_file_or_terminal_methods` and
`no_source_converts_between_the_two_terminal_message_sets` — the 22 rpcs and the converters, which
leave in the milestone that removes them. The "8 failing tests" this section originally claimed was
also wrong: node 6 added **6** tests, of which 2 failed, and both predecessors' inherited reds have
since been fixed by their own owners.

CI also settles the local noise: the 37 failures measured on this machine are **environmental**
(the documented sandbox `self_arc` family, no LiveKit testkit, and the sqlx model store), and CI's
proper environment passes all of them — 6560 of 6562.

### The shared types file arrived here, for one enum

Node 1's changeset already corrected the plan's claim that it would introduce `types.proto`. Node 6
is where one is genuinely needed — and it holds **exactly one type**, `HostDocumentScope`, because
`connection.ConnectionService`'s `StartSession` reaches it too: a session start names the staged
attachments to materialise, and each carries a scope. Node 6's own terminal family (K) turned out to
be a **fourth** fully self-contained cut, sharing nothing with anything.

A test pins the file at one declaration, so anything added later has to be reached by two
really-served services, established the same way.

`connection.proto` keeps its own copy of the enum until the green phase removes the moved methods and
repoints it — two definitions in two packages is legal, and repointing a staying family now would put
that change in a red-phase commit.

The known pre-existing failure inherited from node 1's baseline is expected to stay at exactly one.

## Correction to node 1's backlog entry

`docs/dev/todo/2026-09-09-tonic-adapters-are-hand-written-per-service.md` (node 1) predicts:

> The cost is linear in the `#unbundle` stack: every remaining node that splits a service out of
> `connection.ConnectionService` writes another one.

**That is wrong, and it is the sentence that nearly got the generator dropped.** An adapter is needed
only for a service kept reachable on the local UDS socket; everything else is served through
`ServiceEntry` on Connect-HTTP, LiveKit or stdio and needs none. Nodes 2, 3, 4 and 5 wrote **zero**
adapters — their subsystems' services were already separate protos and were never on that socket.
Measured over production callers alone, only two methods are dialled there at all
(`tddy-sandbox-app/src/daemon_client.rs`: `start_session` and `mint_local_token`, both node 9's),
which sizes the whole remaining debt at ~9 methods and makes a generator a clear loss.

What that measurement missed is the **precedent**: families that were on `ConnectionService` *were*
UDS-reachable, and node 1 kept its two reachable rather than silently dropping that. A node following
the same precedent pays per method of its own families, not per production caller — which is 22 for
node 6.

So the entry's conclusion (do it) is right and its reasoning (linear per node) is wrong, in a way
that would mis-size the work for whoever picked it up. Node 6 **closes** the entry, and the
release-note file records why the prediction did not hold.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-session-io-services.md` — the release-note file,
      carrying the 22 moved coordinates, the deleted converters, and the generator with why the
      "linear per node" prediction did not hold
- [ ] **Close** `docs/dev/todo/2026-09-09-tonic-adapters-are-hand-written-per-service.md`
- [ ] New `docs/dev/todo/` entry: replace node 1's two hand-written adapters with generated ones
- [ ] Move `packages/tddy-daemon/docs/` context and attachment docs to `tddy-session-files/docs/`
- [ ] `packages/tddy-daemon/docs/connection-service.md` — remove 22 endpoint entries
- [ ] `docs/ft/daemon/terminal-sessions.md`, `agent-context-sync.md`, `docs/ft/coder/session-attachments.md`,
      `docs/ft/web/session-files-inspector.md`, `terminal-replay-lazy-scroll.md` — new coordinates
- [ ] Close the two terminal-surface TODO entries
- [ ] New `docs/dev/todo/` entry: make `tddy-coder`'s session participant a real trait implementation
- [ ] Doc triage: `grep -rn -e 'StreamTerminalOutput' -e 'GetTerminalHistory' -e 'StreamContextManifest' -e 'ReadHostDocument' packages/*/README.md packages/*/docs docs/ft`

## Green-phase corrections

Every `## Dependencies` row was re-verified against the tree before implementation, per the pattern
that caught plan-vs-tree drift on nodes 3, 4 and 5. **The dependency gate passed**: node 1's
`types.proto`, its `tddy-daemon-kernel::HOST_DOCUMENT_FRAME_BYTES` and `move_module_to_crate`, and
node 5's `pty_relay` + terminal bridge in `tddy-terminal-rpc` are all present with the shapes this
node was promised. The corrections below are to **this node's own** plan and red phase.

### The proto claims hold; the Rust surface claims did not

Unlike nodes 3 and 5, this node's proto work is accurate: `session_files.proto` declares all 13
rpcs, its single cross-file reference (`types.HostDocumentScope`) resolves, and `types.proto` holds
exactly one declaration as the test pins. `terminal_session.proto`'s 9 rpcs match family K name for
name, and all 17 terminal messages are field-for-field identical. What was wrong was the Rust:

| Claim | Reality |
|---|---|
| Converters at `connection_service.rs:439-457` | They are at **:206** (`to_bridge_terminal_input`) and **:226** (`to_connection_output`); 439-457 is an unrelated notification relay |
| "Both hand converters" — two | **Six.** Two named in the daemon (4 call sites in `rpc_service.rs`) plus **four anonymous inline** in `tddy-coder/src/session_participant/mod.rs:360-372, 392-405, 428-440, 456-462` |
| `tddy-coder` dispatches on `("connection.ConnectionService", method)` | It matches on `method` alone and **discards** the service name (`mod.rs:165`). Splitting the coordinate needs **two `RpcService` impls**, not a renamed string; the name is bound at `mod.rs:108` and `:157` |
| `tddy-coder` serves family K | It serves **7 of 9**. `StreamSessionTerminalIO` and `WatchTerminalControl` fall through to `unimplemented` |
| 3 PTY modules move | **2 of 3.** `pty_runtime.rs` has zero real internal coupling (both its `crate::` paths are shims onto `tddy-pty`/`tddy-daemon-kernel`) and `pty_registry.rs` is a 6-line re-export. `terminal_session_adapter.rs` names `crate::cli_session_manager::{CliSessionManager, PtyHandle}`, and `## Boundaries` pins `cli_session_manager` in the daemon |
| `session_files.proto:12` — the four families share `SessionAttachment`, `StagedAttachmentRef`, `HostDocumentRef` | None of the three is defined in or referenced by this proto; all three are `connection.proto`'s, reached by `StartSession`, which stays. The wire contract is fine; the justification is not |
| File line counts | Stale by 40-90 lines each: `host_documents.rs` 645 (not 595), `context_sync.rs` 634 (595), `session_context_docs.rs` 639 (558), `session_attachments.rs` 663 (570) |
| "8 failing tests define this node" | Node 6 added **6** tests to `unbundle_service_split.rs`, of which **2** fail; `tddy-session-files` has **3** real failures plus 2 tautological ones (one asserts `48*1024 == 48*1024`, the other asserts on the test's own stub) |

### The red phase's `tddy-session-files` surface contradicts the code it is a home for

`ContextSource` as declared (`scope` + framed `Vec<Vec<u8>>` + `SessionFilesError`) collides with the
**real** trait of the same name at `context_sync.rs:33-39`, which is `manifest() -> Result<ContextManifest, Status>`
plus `read(&str) -> Result<Vec<u8>, Status>` — no scope, no framing, `Status` errors, and a `manifest`
method the declaration omits. `DocumentScope` duplicates the generated proto enum while dropping its
`_UNSPECIFIED = 0` variant, and `SessionFilesError` is not the moved code's error type: 9 of the 10
modules return `tddy_rpc::Status`, and `types.proto` pins `FAILED_PRECONDITION` for the
incomplete-upload refusal. `build_session_files_entry`'s two parameters are also insufficient — the
real handlers additionally need a staging base dir, an OS-user resolver, a `project_storage`
main-repo lookup and `max_attachment_bytes`. Per the decision taken on nodes 3 and 5, the green
phase **mirrors the real code** rather than implementing the invented shapes.

### Four decisions taken at green

1. **The `context_files` ↔ `context_sync` ↔ `split_session` cycle is cut here.** `context_files.rs:113`
   calls `split_session::paired_agent`, and `split_session.rs` (which stays) calls back into
   `context_sync`'s `ContextSource`/`ContextSyncer`/`LocalWorktreeSource` at four sites. The cut is
   small because `paired_agent` is a **pure accessor over `tddy_core::SessionMetadata`** — two trimmed
   optional fields, no other coupling — so it moves to `tddy-core`, beside the type it reads. That
   inverts the edge: `split_session` then depends on the new crate, which is the correct direction,
   and all 10 modules move. Only two real callers exist (`context_files.rs:113`,
   `agent_roster.rs:331`).
2. **`generate_tonic_adapter` gains a tonic-trait-path config field, and `to_tonic_status` moves to
   `tddy-service`.** `generate_tonic_adapter: true` is *already* set for `echo_service.proto`
   (`build.rs:71`) and `token.proto` (`:133`), whose `*TonicAdapter` types are publicly re-exported —
   yet neither proto has a tonic pass, so there is no trait to implement. Absent the new field the
   generator keeps today's struct-and-`new()` shape, so those two builds and their re-exports survive.
   `to_tonic_status` had to move because generated adapters land in `tddy-service`'s and
   `tddy-terminal-rpc`'s `OUT_DIR` and neither depends on `tddy-daemon`; `tddy-rpc`'s own conversion
   pins **tonic 0.11** against everything else's **0.12**, which is why the hand-written one exists.
   Re-pointing the three adapters' imports is a one-line edit each and is **not** the retro-fit
   `## Boundaries` forbids.
3. **Parity is asserted over the 7 methods both servers actually serve**, with the 2 the coder never
   served recorded as a `docs/dev/todo/` entry. Adding a bidirectional `StreamSessionTerminalIO` to
   the session participant is net-new behaviour, not the relocation of family K.
4. **The web migration stays in this PR.** The cross-package proto generation is *not* unsolved:
   `scripts/generated-code.manifest` already supports several proto roots per package, and
   `packages/tddy-livekit-web` proves it (`../tddy-service/proto --path …/terminal.proto` produces its
   `terminal_pb.ts`). `session_files_pb.ts` and `types_pb.ts` need no new plumbing at all — both
   protos are already in the root `tddy-web` generates from — and `terminal_session_pb.ts` needs one
   manifest line for `../tddy-terminal-rpc/proto`.

### Two hazards the plan does not mention

- **`connection_service_keeps_exactly_the_methods_node_one_leaves_behind` pins the rpc count and
  passes today, so it flips red *because* this node succeeds.** The literal is a running total every
  node has to bump: it was `73` when this node's green phase started, and node 4's
  `StreamLiveKitRooms` removal landing mid-flight took it to `72`. Removing this node's 22 takes it
  to **50** — which is exactly what `## Technical Changes` § State B predicted, so the plan's
  end-state number was right all along and only the intermediate was off. Bumped rather than
  computed from the proto: a count derived from its own input asserts nothing, whereas a literal
  still catches removing 21 or 23 by accident. It carries a comment saying each node updates it.
  Node 4's inherited red (below) is **resolved** as of this rebase.
- **The converter-absence sweep is satisfiable by deleting a doc comment.** It greps one needle
  (`connection::SessionTerminalInput`) under `packages/tddy-daemon/src` only, so it misses the second
  converter entirely and every one of the coder's four inline copies. The green phase widens it to
  both message names and both packages, so it enforces what it claims.

### Two inherited reds arrive on this branch from predecessors

Both are **outside this node's `## Responsibility`** and are reported rather than fixed here, since
implementing a predecessor's owned symbol is the duplicate-development failure `## Dependencies`
exists to prevent. They will nonetheless show in this PR's CI, so they are recorded to stop them
being read as node 6 damage:

1. **Node 4 (#473)** — *resolved while this node was in green.*
   `connection_service_no_longer_declares_the_rooms_stream`, node 4's own completion criterion,
   failed on arrival with `StreamLiveKitRooms` still declared in `connection.proto`. It has since
   landed: the rpc no longer appears, the count literal moved 73 → 72, and the pre-node-6 total is
   now 72. Recorded because it is the second predecessor red that fixed itself mid-flight, which is
   the argument for re-verifying inherited failures after every rebase rather than carrying the
   first reading forward.
2. **Node 5 (#474)** — *resolved while this node was in green.* At the first rebase,
   `session_tool_client::tests::refuses_a_call_on_a_session_with_no_transport` panicked with
   `not implemented: dispatch_session_tool` at `packages/tddy-service/src/session_tool_client.rs:79`,
   node 5's owned surface. Node 5 then force-pushed a rewritten history ending in `4baa3514`
   ("a crate for the session tool client, and the three dependency drops"), which implements it:
   `cargo test -p tddy-service --lib` is now **104 passed / 0 failed**. Recorded because it is the
   concrete case for the rule below — the base moved twice during one green phase.


### The base moved twice during this green phase

Node 5 force-pushed a rewritten history *while milestone 1 was building*, so the branch went stale
between the step-0 rebase and the first milestone push — the push was rejected non-fast-forward, not
because anything local was wrong. Both rebases used
`git rebase --onto <new base> <recorded pre-rebase tip>`, which is what keeps a predecessor's old
commits from being replayed as this node's own; a plain `git rebase` at that moment would have
duplicated node 5's entire delta into this PR's diff. Every milestone here re-reads the base tip
immediately before pushing rather than trusting the tip step 0 saw.

### The test-suite split: 5 of 14 moved

The changeset promised the moved suites would "carry most of the proof" and move with the code.
Five could; nine could not, each pinned by a symbol that stays in `tddy-daemon`. Moving any of them
would put `tddy-daemon` back on `tddy-session-files`' dependency path and defeat the extraction —
the same measurement node 4 made (5 of 18 there).

**Moved**, assertions untouched — the whole content diff is import paths:
`context_file_frames_unit.rs`, `context_files_acceptance.rs`,
`pr_stack_child_doc_attachment_acceptance.rs`, `pr_stack_context_docs_acceptance.rs`,
`staged_attachment_path_validation.rs`.

**Stayed**, with the pinning symbol:

| Suite | Pinned by |
|---|---|
| `context_sync_acceptance.rs` | `split_session::build_split_context_dir` |
| `session_workflow_files_rpc.rs` | `test_util::{test_service, TEST_TOKEN}` |
| `session_file_upload_rpc.rs` | `connection_service::ConnectionServiceImpl`, `test_util::TEST_TOKEN` |
| `session_uploads_rpc.rs` | `connection_service::ConnectionServiceImpl`, `test_util::TEST_TOKEN` |
| `staging_rpc_acceptance.rs` | `connection_service::ConnectionServiceImpl` |
| `staging_forwarding_acceptance.rs` | `connection_service::ConnectionServiceImpl`, `runtime::spawn_common_room_discovery_task` |
| `session_attach_staging_scope_acceptance.rs` | `connection_service::ConnectionServiceImpl` |
| `session_attach_cross_host_acceptance.rs` | `connection_service::ConnectionServiceImpl`, `multi_host::EligibleDaemonSource` |
| `session_room_acceptance.rs` | `connection_service::ConnectionServiceImpl`, `split_session::prepare_split_agent_wiring` |

All nine pass where they are.

### Peer routing stays in the daemon

`build_session_files_entry` serves all 13 methods, but `daemon_instance_id` peer routing is
deliberately **not** in the crate: it needs `classify_daemon_route`, `common_room_slot` and the
per-method `forward_*_via_livekit` clients, which are the daemon's transport layer.
`connection.ConnectionService` still serves these 13 with their routing intact, so nothing
regressed — but whoever registers this entry has to wrap it. Recorded in `service.rs`'s module
header rather than as a `TODO`, because it is a boundary statement, not deferred work in this crate.

### Baseline, measured on both sides

The recorded "1027 passed / 1 failed" is a **fail-fast** number — `cargo test` stops at the first
failing binary — so it is not comparable to a whole-suite figure, and this node's baseline row was
misleading rather than merely stale. Measured with `--no-fail-fast` on both sides, over 135 binaries:

| | passed | failed | ignored |
|---|---:|---:|---:|
| with this node | 1318 | 37 | 3 |
| base, the 10 failing binaries only | 36 | 37 | 1 |

The failure set is **byte-identical** to the base's 37, in three pre-existing families: the
documented sandbox `self_arc called before set_self_handle` group (see the daemon
`sandbox_behavior_acceptance` note), `Once instance has previously been poisoned` where no LiveKit
testkit is running, and the sqlx model-registry store in this sandbox. **No regressions.**

One extra failure appeared in a first run (`serves_the_rooms_stream_as_its_own_service`, the
model-registry store) and passed both in isolation and on re-run — a flake under 135-binary
parallelism, not this node's.

### File budget: 8 files over 500 lines

`host_documents.rs` 708, `session_context_docs.rs` 639, `context_sync.rs` 634,
`session_attachments.rs` 613, `lib.rs` 563, `stack_doc_attachments.rs` 517, `service.rs` 517,
`context_files.rs` 509. Seven arrived over budget. They were **not** split: a split for line count
alone cuts cohesive units and puts churn on top of a rename, and the diffs are far more reviewable
while they stay pure renames. `host_documents.rs` is the honest refactor target — it grew by the
framing function it absorbed.

### The sandbox terminal branch was a second copy of the bridge

The plan treated family K as nine handlers to relocate. Four of them — `StreamSessionTerminalIO`,
`StreamTerminalOutput`, `SendTerminalInput`, `GetTerminalHistory` — also branched on
`sandbox_manager`, and the `StreamTerminalOutput` branch was a hand-rolled ~90-line reimplementation
of the bridge's replay and offset arithmetic. Its own comment said so: *"matching
`tddy_terminal_rpc::bridge`."* Two implementations of one offset contract, in the surface whose
entire purpose is to be the single terminal surface, and this repo has already been bitten by two
terminal paths disagreeing.

It is unified through `TerminalSessionStore`, which is what that trait was written for: a
`SandboxTerminalSession` adapter in `tddy-daemon` (beside the existing `DaemonTerminalSession`), and
a composite store that resolves a sandbox session first. All four branches are gone, along with 104
further lines in `connection_service.rs` that nothing constructed once they went.

**No predecessor crate was touched.** `SandboxSessionState`'s `stdout_tx`, `capture` — literally the
same `tddy_task::TerminalCapture` arc — and `stdin_tx` are already public, so the adapter maps them
directly; the three the sandbox lacks are synthesised to preserve today's behaviour: the acked-offset
watch is dropped immediately so the bridge emits **no** ACK frames, `resize` is a no-op, and
`pty_done` is derived from the stdout broadcast closing. Editing `tddy-daemon-sandbox` would have
been reaching into node 3's crate.

One defaulted trait method was added: `TerminalSession::resizable() -> bool`, default `true`. It
guards the bridge's post-resize **drain**, which would otherwise have discarded live bytes that no
replay chunk covers whenever a client supplied dimensions for a sandbox session — a real gap, not a
port detail. Defaulting it meant no implementor outside the crate changed, which is why `tddy-coder`
was not touched here.

**Three observable changes for sandboxed sessions, accepted deliberately** (decision taken
2026-09-11) because this node is where the terminal surface is unified or the split is locked in —
the two `docs/dev/todo/2026-07-28-terminal-*` entries are exactly this:

| | Before | After |
|---|---|---|
| `GetTerminalHistory` | `not_found` | real offset-anchored chunks |
| `StreamSessionTerminalIO` | live only — no prologue, replay or anchoring frame | replays like every other terminal |
| `StreamTerminalOutput`, TAIL | the whole retained buffer, 32 KiB frames | prologue + last 8 KiB, scroll-up for the rest |

The third is the only regression-shaped one, and it is only coherent *because* of the first: the
scrollback is still reachable, paged rather than pushed on open, which is how every non-sandboxed
terminal has always behaved. The alternative — keeping a mode branch for sandbox sessions — was
rejected: it leaves a sandbox-specific arm in the one surface that is supposed to have none.

Evidence: `packages/tddy-daemon/tests/sandbox_terminal_parity_acceptance.rs` (10 tests) builds a
**real** `SandboxSessionState` and carries the deleted loop verbatim as an oracle, comparing served
frames against it and against literal expected frames across both replay modes, the prologue,
forward fill, and drifted-offset clamping.

⚠ **An honest gap in the end-to-end evidence.** The two daemon tests that would catch a
sandbox-terminal regression through the full stack —
`sandboxed_claude_cli_terminal_io_round_trips` and
`sandboxed_session_streams_demo_tui_dimensions_in_terminal` — die on the pre-existing
`ConnectionServiceImpl::self_arc called before set_self_handle` harness fault *before reaching any
terminal RPC*, so they neither confirm nor deny this change. That is why the parity suite exists at
the store/adapter level. Fixing that harness gap is not this node's, and it is the reason the
`self_arc` family is worth someone owning.

### Only one of the three PTY modules could move, and for a measured reason

`## Affected Packages` says `tddy-terminal-rpc` "gains the 3 PTY modules". It gains one function.

- **`pty_registry.rs` was deleted, not moved.** Six lines of `pub use tddy_pty::{PtyControl,
  PtyRegistry};` with two importers. Relocating a re-export shim moves nothing; the importers now
  name `tddy_pty` directly.
- **`pty_runtime.rs` stays in `tddy-daemon`.** Its `crate::` paths are shims — the plan was right
  that there is no *daemon* coupling — but the crates behind them are not free:
  `privilege_drop` and `spawn_path_extra_for_home` come from `tddy-daemon-kernel`, which depends
  **non-optionally** on `tddy-livekit`. `tddy-tools` depends on `tddy-terminal-rpc`
  non-optionally and carries a `livekit` feature (`Cargo.toml:75`) whose entire purpose is to keep
  the LiveKit/webrtc SDK out of an in-jail build that only speaks gRPC. Measured:
  `cargo tree -p tddy-tools --no-default-features -e normal | grep -c livekit` is **0** today, and
  moving `pty_runtime` would make it non-zero unconditionally. The extraction is not worth defeating
  that flag.
- **What did move is `login_shell_for_os_user`** → `tddy-terminal-rpc/src/login_shell.rs`, a pure
  passwd lookup with no daemon state, because `StartTerminalSession` is served from this crate now.
  `connection.ConnectionService` was re-pointed at the same function, so the two coordinates cannot
  start a session in different shells while both are mounted.
- **`terminal_session_adapter.rs` stays**, as `## Boundaries` implies: it binds the daemon's own
  managers to the trait, which is what the trait is for, and it is where the sandbox adapter went.

Both coordinates are mounted from the **same** managers in `runtime.rs`, so while
`connection.ConnectionService` still declares family K, the two address one set of PTYs and one
control lease.

### The fourth transport: `tddy-sandbox-app` dials the terminal family over the local UDS socket

Removing the 22 rpcs broke the build on a consumer no document in this stack accounts for.
`tddy-sandbox-app` is the binary that runs **inside every jail**; it connects back to the daemon over
the local UDS socket with tonic and opens the bidirectional terminal stream
(`daemon_client.rs::run_daemon_terminal_bridge`). Checking the daemon, `tddy-coder` and `tddy-web`
was not enough, because consumers of `connection.ConnectionService` are spread across **four**
transports, not three:

1. Connect-HTTP — `tddy-web`
2. LiveKit — `tddy-coder`'s session participant
3. in-process `ServiceEntry` — the daemon itself
4. **the local UDS/tonic socket** — `packages/tddy-daemon/src/local_socket_server.rs`, dialled by
   `tddy-sandbox-app`

**This falsifies the `## Correction to node 1's backlog entry` section above.** That section argues,
against the plan, that *"measured over production callers alone, only two methods are dialled there
at all (`tddy-sandbox-app/src/daemon_client.rs`: `start_session` and `mint_local_token`, both node
9's)"*, and uses it to size the whole remaining adapter debt at ~9 methods. `StreamSessionTerminalIO`
is dialled there too, from the same file — which is precisely why the only bidirectional method in a
90-method surface exists. The section's *conclusion* (build the generator) survives; its second
measurement does not, and the reasoning it corrected was wrong in a different way than it claimed.

Removing the rpcs left the jail with nothing to dial, because the socket had never served the new
coordinate: `local_socket_server.rs` mounted three tonic services, and the removal stripped the
now-dangling terminal stream bounds from `serve_connection_uds`'s `where` clause without adding the
replacement. The symptom was a **compile error in `tddy-sandbox-app`**, not a failing test — so no
amount of test-suite green would have caught it.

The socket now mounts a fourth service, and it is the **generated** adapter: nine delegations
including the bidirectional one, none hand-written. That makes this the first production traffic the
milestone-1 generator carries, and the concrete vindication of putting it in this node rather than
hand-writing adapters. `ConnectionServiceImpl::terminal_session_service()` already returned the typed
impl the adapter needs — the type-erased `ServiceEntry` cannot be wrapped — and both mounts are built
from the same ports over the same `Arc`s, so the socket and the Connect-HTTP entry address one set of
PTYs and one control lease.

**Process note worth carrying to nodes 7, 8 and 9:** run `cargo build --workspace` once before any
proto removal. It is the documented exception to this repo's scope-it-locally rule — a removal can
break any package, and a two-minute local sweep beats a 25-minute CI round trip that only reveals the
first broken one. Eight of this node's milestones were verified locally and CI reported `Rust lint`
and `Rust build` green on every one; the ninth was pushed with its gates interrupted, and it is the
one that broke the build.

### The UDS premise cut both ways, and it halves this node's own cost argument

Two separate claims about the local UDS socket were wrong, in opposite directions, and together they
resize the generator's justification.

**Too few:** the `## Correction to node 1's backlog entry` section measured only `start_session` and
`mint_local_token` as dialled there. `StreamSessionTerminalIO` is dialled too, from the same file —
which is why removing family K broke the build, and why the only bidirectional method in a 90-method
surface exists.

**Too many:** that section also argued node 6 would owe **22** hand-written adapter methods, by
following node 1's precedent of keeping every family it moved UDS-reachable. Only **9** were actually
needed. Nothing dials a session-file method over that socket — checked across both in-jail binaries —
so mounting `session_files` there would have added surface with no caller, which is the failure this
node spent a milestone fixing in the other direction. The terminal service is on the socket because
the jail provably dials it; the file service is not because nothing does.

So the arithmetic that flipped the decision to build the generator was itself wrong: the real figure
was 9 methods, not 22 — close to the ~9 the section dismissed as "a clear loss". **The generator is
still the right call, for the reason the section gave second rather than first:** those 9 include the
bidirectional method, a generator built anywhere else would have been discovered incomplete the first
time it met one, and it is now what serves the jail's terminal traffic in production. The capability
argument carries this node; the cost argument does not.

### The sandbox hop got a better answer than the plan's

`## Scope` called for re-pointing the sandbox pass's `.connection.SessionTerminalOutput` extern path
at `terminal_session`'s message. That is not possible: `tddy-terminal-rpc` depends on `tddy-service`,
so naming its message from `sandbox.proto` is a dependency cycle.

`sandbox.proto` instead owns a `SandboxTerminalOutput` carrying only what crosses that hop. The four
fields it drops — `acked_input_offset`, `start_offset`, `end_offset`, `at_oldest` — are capture-ring
and input-ack metadata the jail never sets and the host relay never reads, and they are `reserved` so
a later field cannot claim one and collide with what an older runner still encodes. The three
surviving fields keep their numbers, so the bytes on that hop are unchanged. The borrowed message was
the anomaly; this removes it rather than re-pointing it.

## Validation Results

### Test Fixes (/fix-tests)

**Last Run**: 2026-09-11
**Status**: ✅ All 23 fixed, plus 2 uncovered regressions closed

**Summary**:
- Suites diagnosed: 4 (each isolated; cross-host suites re-run alone, since they bind LiveKit ports)
- Root causes: **3 distinct**, not one
- New tests added: 4. Tests weakened or deleted: **0**
- All fixes verified against the production symptom, not the assertion

| Suites | Class | Root cause | Outcome |
|---|---|---|---|
| `session_attach_cross_host` (5) | production | the routing fork sat on the **transport**, so `session_files_service()` returned an unrouted impl and every in-process caller was served locally whatever `daemon_instance_id` it named | routing moved onto the generated `SessionFilesService` trait — ✅ 8/8 |
| `remote_managed_worktree_cross_host`, `session_room_cross_host`, `split_session_resume` (18) | test infrastructure | each suite had its **own** `serve_rpc_participant` mounting only `connection.ConnectionService`, so forwarded calls reached a peer that did not serve them | one shared `test_util` helper across all 4 call sites — ✅ 10/10, 4/4, 8/8 |
| context handlers | production, **uncovered** | the move dropped `tokio::time::timeout`; a stalled read hung the RPC forever instead of answering `DEADLINE_EXCEEDED` | deadline restored as a port; 3 tests — ✅ |
| split context read | production, **uncovered** | the caller re-implemented the read when the handler moved into the crate, losing the deadline, the gate and the single path | duplicate **deleted**, reads through the served surface; 1 test — ✅ |

### Key insights worth keeping

**Two tests were passing for the wrong reason, which is worse than the five that failed.**
`stream_read_host_document_forwards_to_the_peer_that_owns_the_document` staged onto what it believed
was the peer, read back from the same host, and asserted success **without a byte crossing**. The
green run of this suite recorded during the session-files milestone was therefore partly hollow. A
cross-host suite can pass while proving nothing, so "the suite is green" is not evidence that
forwarding works — only an assertion on the *peer's* state is.

**Four silent losses came out of one 3,500-line relocation**, and every one was invisible to the
suite as it stood: the in-jail UDS break (a compile error, no test), the unrouted in-process calls,
and two dropped deadlines. Three of the four were surfaced by CI or by an implementer checking a
premise, not by the verification run for the milestone that caused them. The pattern is specific:
**a mechanical call-site substitution silently changes which layer answers.** Both the UDS break and
the routing break were `X.method(...)` → `X.something().method(...)` rewrites that compiled and
type-checked perfectly.

**Moving routing up also fixed an ordering the wrapper had inverted.** The five staging methods now
authenticate *before* classifying, as `connection.ConnectionService` did; the transport wrapper
classified first, which let an unauthenticated request drive an outbound forward — the exact
inversion `stream_start_session_refuses_an_invalid_token_before_it_classifies_the_route` exists to
forbid. That test did not catch it because it covers `StartSession`, which never moved.

**Deadline tests need a deterministic stall, not a sleep.** Both new suites use a current-thread
runtime with `max_blocking_threads(1)`, that one thread occupied by a read parked on a channel until
the assertion is made, so the code under test's own `spawn_blocking` cannot start whatever the
machine is doing. The obvious seam — a `ContextSource` double — does not work, because that trait
belongs to the syncer and no handler goes through it; nor does a FIFO, since both readers gate on
`is_file()` and refuse one rather than blocking.

### Environment notes

- The shared `tddy-livekit-testkit` container can be **unusable** while appearing healthy: its
  LiveKit advertises `127.0.0.1:7881`/`7882` for ICE while Docker maps those elsewhere, so every
  participant fails at connect with `wait_pc_connection timed out` — 8 tests "failing" in 542s with
  no assertion reached. Unsetting `LIVEKIT_TESTKIT_WS_URL` lets testcontainers start a correctly
  mapped one, and the suites run in 20-90s.
- The cross-host suites start two daemons against one LiveKit container and are genuinely
  load-sensitive: one run of `session_attach_cross_host` failed 1/8 with two tests exceeding 60s and
  passed 8/8 on re-run. Run them individually and re-run a failure in isolation before believing it.
