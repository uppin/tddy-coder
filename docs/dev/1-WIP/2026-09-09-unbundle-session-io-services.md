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

- [ ] **Proto**: `session_files.proto` (13 methods); `connection.proto` loses 22 rpcs; vacated field numbers `reserved`
- [ ] **Tonic adapter generator**: `tddy-codegen`'s `generate_tonic_adapter` implemented — unary,
      server-streaming (with the associated `…Stream` type) and **bidirectional**; emits calls to the
      shared status conversion rather than inlining its own
- [ ] **Generated adapters used**: this node's two services reach the UDS socket through generated
      adapters, not hand-written ones
- [ ] **⛔ Terminal surface unified**: `terminal_session.TerminalSessionService` served; **both hand converters deleted**
- [ ] **`tddy-session-files`**: crate, 10 modules, its suites
- [ ] **`tddy-terminal-rpc`**: serves family K; gains the 3 PTY modules
- [ ] **⛔ `tddy-coder` lockstep**: its session participant moves to the new terminal coordinate in this PR
- [ ] **Sandbox extern path**: the `.connection.SessionTerminalOutput` remap re-pointed
- [ ] **Web**: 11 hooks and components migrated; the Cypress terminal and file fakes split
- [ ] **File budget**: record which over-500-line files landed under budget and which did not, with why
- [ ] **Baseline**: `./test` per touched package back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
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

- [ ] M1 — `session_files.proto` generates; `types.proto` imported rather than duplicated
- [ ] M2 — `tddy-session-files` extracted; its suites pass
- [ ] M3 — `terminal_session.TerminalSessionService` served from `tddy-terminal-rpc`; PTY modules moved
- [ ] M4 — both hand converters deleted; `grep -rn 'connection.SessionTerminalInput'` finds nothing
- [ ] M5 — `tddy-coder`'s participant moved; HTTP and LiveKit answer a session identically
- [ ] M6 — sandbox extern path re-pointed; `cargo build -p tddy-service` clean
- [ ] M7 — web migrated; Cypress component suites green; baselines restored; file budget recorded

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
- [ ] **Integration**: all 9 `terminal_session.TerminalSessionService` methods answer over Connect-HTTP (`terminal_session_service_acceptance.rs`)
- [ ] **Integration**: `StreamSessionTerminalIO` carries a bidi session end to end (`terminal_session_bidi_acceptance.rs`)
- [ ] **Integration**: `GetTerminalHistory` returns the same frames at the same offsets as the old coordinate (`terminal_history_parity_acceptance.rs`)

### tddy-terminal-rpc + tddy-coder
- [ ] **Integration**: the same session answers identically through the daemon's server and through
      `tddy-coder`'s session participant — history offsets, stream mode, control claim (`two_server_parity_acceptance.rs`)

### tddy-codegen
- [ ] **Unit**: a unary method generates an `async fn` that unwraps `tonic::Request`, delegates, and maps the result (`generator.rs`)
- [ ] **Unit**: a server-streaming method generates the associated `…Stream` type as well as the method (`generator.rs`)
- [ ] **Unit**: **a bidirectional method generates a `Streaming` request and a stream response** — the case only family K has (`generator.rs`)
- [ ] **Unit**: the generated body calls the shared status conversion rather than constructing a `tonic::Status` itself (`generator.rs`)
- [ ] **Integration**: a generated adapter serves the same responses as the hand-written one it replaces, for one service (`generated_adapter_parity_acceptance.rs`)

### tddy-session-files
- [ ] **Integration**: all 13 `session_files.SessionFilesService` methods answer (`session_files_service_acceptance.rs`)
- [ ] **Integration**: context manifest and batched context reads stream identically to the old coordinate (`context_sync_acceptance.rs`)
- [ ] **Integration**: a staged attachment uploads, lists, materialises and deletes (`staging_rpc_acceptance.rs`)
- [ ] **Integration**: `StreamReadHostDocument` frames at `HOST_DOCUMENT_FRAME_BYTES` from the kernel (`host_document_frames_unit.rs`)

### tddy-service
- [ ] **Unit**: no source file references `connection.SessionTerminalInput` or `connection.SessionTerminalOutput` (`converter_absence_unit.rs`)
- [ ] **Unit**: the sandbox tonic pass compiles with the re-pointed extern path (build-level)

### tddy-web
- [ ] **Cypress component**: a terminal opens, receives output and accepts input at the new coordinate (`GrpcSessionTerminal.cy.tsx`)
- [ ] **Cypress component**: the session files tab lists and deletes uploads (`SessionFilesTab.cy.tsx`)

### tddy-daemon
- [ ] **Integration**: `connection.ConnectionService` declares 50 methods and none of the 22 (`service_registration_acceptance.rs`)

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
- [ ] Field numbers vacated in `connection.proto` are `reserved`, so the schema carries the history of
      this split permanently — intended, but it makes `connection.proto` harder to read

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | |
| `cargo clippy -p tddy-session-files -p tddy-service --all-targets -- -D warnings` | ✅ exit 0 | |
| `./dev bun run --filter tddy-web cypress:component` | not yet run — no web change in commit 2 | |

**8 failing tests** define this node: 3 in `tddy-session-files` and 5 in `tddy-service` (the
inherited ones plus `session_files`/terminal shape and the converter-absence sweep).

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

- **`unbundle_service_split.rs:139` pins `ConnectionService` at exactly 73 rpcs and passes today.**
  Removing 22 takes it to 51, so that literal must move or the test flips red *because* this node
  succeeded. Worse, 73 is only correct because node 4's `StreamLiveKitRooms` removal never happened —
  `connection_service_no_longer_declares_the_rooms_stream` (node 4's criterion) is **failing on
  arrival** at `connection.proto:283`, and `tddy-web/src/gen` has no `livekit_pb.ts`. The count is
  computed from the proto rather than hand-bumped, and node 4's inherited red is reported, not fixed
  here.
- **The converter-absence sweep is satisfiable by deleting a doc comment.** It greps one needle
  (`connection::SessionTerminalInput`) under `packages/tddy-daemon/src` only, so it misses the second
  converter entirely and every one of the coder's four inline copies. The green phase widens it to
  both message names and both packages, so it enforces what it claims.

### Two inherited reds arrive on this branch from predecessors

Both are **outside this node's `## Responsibility`** and are reported rather than fixed here, since
implementing a predecessor's owned symbol is the duplicate-development failure `## Dependencies`
exists to prevent. They will nonetheless show in this PR's CI, so they are recorded to stop them
being read as node 6 damage:

1. **Node 4 (#473)** — `connection_service_no_longer_declares_the_rooms_stream`, node 4's own
   completion criterion, fails on arrival: `StreamLiveKitRooms` is still declared at
   `connection.proto:283`, and `packages/tddy-web/src/gen` has no `livekit_pb.ts`. This is also what
   makes the `73` literal at `unbundle_service_split.rs:139` arithmetically correct today — when node
   4 finishes, the pre-node-6 count is 72.
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
