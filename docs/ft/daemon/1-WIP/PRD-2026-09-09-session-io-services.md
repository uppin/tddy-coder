# PRD: The terminal, context and session-file services

**Date**: 2026-09-09
**PRD Type**: Architecture Change (breaking RPC change — 22 methods)
**Product Area**: daemon
**Stack**: `#unbundle` node 6 of 8

## Affected Features

- [terminal-sessions.md](../terminal-sessions.md) — family K moves to the service that was already
  written for it and never served
- [agent-context-sync.md](../agent-context-sync.md) — family J moves
- [session-attachments.md](../../coder/session-attachments.md) — family S moves
- [session-files-inspector.md](../../web/session-files-inspector.md) — family R moves
- [terminal-replay-lazy-scroll.md](../../web/terminal-replay-lazy-scroll.md) — its `GetTerminalHistory`
  coordinate moves
- [remote-managed-worktree.md](../remote-managed-worktree.md) — context sync moves
- [rpc-playground.md](../rpc-playground.md) — the picker gains two services and loses 22 methods

## Summary

**Two deliverables.** The families, and the codegen that stops their adapters being hand-written.



22 of `ConnectionService`'s remaining 72 methods leave: the terminal family (K, 9 methods), agent
context sync (J, 3), session workflow files (I, 2), session uploads (R, 3) and staged attachments plus
host documents (S, 5). Terminals go to **`terminal_session.TerminalSessionService`, a service that
already exists, is a byte-for-byte duplicate of family K, and is served nowhere.** The other 13
methods become `session_files.SessionFilesService`, served by a new `tddy-session-files` crate.

### `tddy-codegen`'s `generate_tonic_adapter` is implemented here

Every service kept reachable on the local UDS socket needs a tonic adapter, and the generator that
should write them is a stub — it emits an adapter struct and a `new()`, and nothing else. Node 1
therefore hand-wrote two adapters, 17 `async fn`s. Node 6, keeping its two services reachable on the
same precedent, would hand-write **22**.

Two facts make this the node to fix it in rather than a later one:

- It is the **largest** hand-writing cost of any node.
- `StreamSessionTerminalIO` is the **only bidirectional method in the whole 90-method surface** —
  69 unary, 20 server-streaming, 1 bidi. A generator built anywhere else would handle two of the
  three shapes and be found incomplete later; here it cannot be.

A declarative macro is not an option: `#[tonic::async_trait]` rewrites the signatures of the trait it
is applied to, and a macro cannot see through that rewrite to generate the bodies.

## Background

### The terminal family is already extracted and wired to nothing

`packages/tddy-terminal-rpc/proto/terminal_session.proto` declares `TerminalSessionService` with **9
rpcs that duplicate family K exactly**. Its header says it *"Consolidates the terminal streaming
bridge logic that was previously duplicated between the two"*, and its `build.rs` mirrors
`tddy-service/build.rs`'s two-pass pattern.

`grep -rn 'TerminalSessionService\|terminal_session\.'` across the repo, excluding that package,
returns **zero hits.** What is actually shared is the *bridge* —
`serve_stream_terminal_output_with`, `serve_get_terminal_history_with`,
`serve_stream_session_terminal_io_with` and the `TerminalSession`/`TerminalSessionStore` traits — called
from `connection_service.rs:14506` and `tddy-coder/src/session_participant/mod.rs:374,441`. And those
call sites **hand-convert** `connection.SessionTerminalInput` → `terminal_session.SessionTerminalInput`
at `connection_service.rs:439-457`.

So the repo already paid for the extraction and got a duplicate schema, two hand-written converters
and no served coordinate. **This node finishes it**, and that history is the single strongest argument
in the stack for why a proto split must move the *served coordinate*, not just the schema.

### Two servers, one surface

`tddy-coder`'s session participant is a **second** server for families K, L, M and N — a string
dispatch over `("connection.ConnectionService", method)` that returns `Unimplemented` for everything
else, with the web routing to whichever participant can answer. The repo has already been bitten by
letting the two drift:

> the session participant serves tail mode and the cursor in lockstep with the daemon … had it not,
> the same session would have opened tail-first when reached over HTTP and head-first when reached
> over LiveKit.

**Family K must move in `tddy-coder/src/session_participant/mod.rs` in this same PR**, or the same
session answers differently depending on which transport reaches it.

## Proposed Changes

### What changes

| New coordinate | Methods | Served by |
|---|---|---|
| `terminal_session.TerminalSessionService` *(exists, unserved)* | family K's 9 | `tddy-terminal-rpc`, using the bridge it already owns |
| `session_files.SessionFilesService` *(new)* | families I (2), J (3), R (3), S (5) | `tddy-session-files` *(new crate)* |

The Rust source moves with the surface: the context/documents subsystem (`host_documents.rs`,
`context_sync.rs`, `session_context_docs.rs`, `context_files.rs`, `stack_doc_attachments.rs`,
`session_workflow_files.rs` — 2,197 prod LoC), the attachment and upload modules
(`session_attachments.rs`, `session_attachment_staging.rs`, `session_file_upload.rs`,
`session_uploads.rs` — 1,200), and the PTY modules (`pty_runtime.rs`, `terminal_session_adapter.rs`,
`pty_registry.rs` — 445).

**Both hand-written converters are deleted**, and `terminal_session.proto`'s messages become the only
ones.

### What stays the same

- Every terminal, context and file behaviour. `restructure verify --against <ref>` proves the
  statement multiset is unchanged.
- `cli_session_manager.rs` stays in the daemon. It is the PTY *session lifecycle* and the origin of
  the `TaskRegistry` that five services share; family C keeps it.
- The remaining 50 `ConnectionService` methods, until nodes 7 and 8.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-daemon` | −13 modules, −3,842 prod LoC; two `ServiceEntry` groups; the two converters deleted |
| `tddy-terminal-rpc` | serves the service it has owned unserved since it was created |
| `tddy-coder` | `session_participant/mod.rs`'s family-K dispatch **must** move in lockstep |
| `tddy-service` | `session_files.proto` appears; `connection.proto` loses 22 rpcs; `terminal_session.proto`'s messages become canonical, so the `.connection.SessionTerminalOutput` extern path in the sandbox tonic pass re-points again |
| `tddy-web` | terminal, context, attachment and upload call sites; `terminalFeed.ts`, `GrpcSessionTerminal.tsx`, `useSessionTerminals.ts`, `useTerminalControl.ts`, `terminalHistoryLoader.ts`, `worktreeFilesApi.ts`, `useSessionFileUpload.ts`, `useStagedAttachmentUpload.ts`, `HostDocumentPicker.tsx`, `SessionFilesTab.tsx`, `SessionWorkflowFilesModal.tsx` |
| `tddy-tools` | `pty_relay` (moved to `tddy-terminal-rpc` by node 5) names `SendTerminalInput` and `StreamTerminalOutput`; those coordinates change |

### User-facing

22 RPC coordinates move — the largest single break in the stack. A web bundle from before this node
cannot open a terminal, sync context, or upload a file.

## Implementation Plan

1. `session_files.proto` created with families I, J, R and S, importing `types.proto`.
2. `tddy-session-files` extracted with the 10 context/attachment/upload modules.
3. `terminal_session.proto`'s existing service is served from `tddy-terminal-rpc`; the PTY modules move.
4. Both hand converters deleted; `connection.proto`'s terminal messages removed; the sandbox extern
   path re-pointed.
5. `tddy-coder`'s session participant moved to the new terminal coordinate — **same commit as (3)**.
6. `tddy-web` migrated; the Cypress terminal and file fakes split.

## Acceptance Criteria

- [ ] `generate_tonic_adapter` emits a working tonic trait impl for **unary**, **server-streaming**
      (including the associated `…Stream` type) and **bidirectional** methods
- [ ] the generated body calls the shared status conversion rather than constructing its own
- [ ] this node's two services reach the UDS socket through **generated** adapters
- [ ] a generated adapter answers identically to a hand-written one for the same service

- [ ] `terminal_session.TerminalSessionService` serves all 9 family-K methods over Connect-HTTP,
      LiveKit and the local UDS socket
- [ ] `session_files.SessionFilesService` serves all 13 methods of families I, J, R and S
- [ ] `connection.ConnectionService` declares none of the 22, and is down to 50 methods
- [ ] `grep -rn 'connection.SessionTerminalInput'` finds nothing — both converters are gone
- [ ] `tddy-coder`'s session participant serves family K at the new coordinate, and a session reached
      over LiveKit and over HTTP answers identically
- [ ] the sandbox tonic pass's terminal extern path resolves
- [ ] `tddy-web` opens a terminal, replays history, syncs context and uploads a file at the new coordinates
- [ ] `./test -p tddy-daemon -p tddy-session-files -p tddy-terminal-rpc -p tddy-coder` matches baseline

## References

- Changeset: [2026-09-09-unbundle-session-io-services.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-session-io-services.md)
- Discovery: [2026-09-09-unbundle-session-io-services-initial-discovery.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-session-io-services-initial-discovery.md)
- Node 1's PRD: [PRD-2026-09-09-host-worktree-services.md](./PRD-2026-09-09-host-worktree-services.md)
