# PRD: Host and worktree services in their own crates

**Date**: 2026-09-09
**PRD Type**: Architecture Change (breaking RPC change)
**Product Area**: daemon
**Stack**: `#unbundle` node 1 of 8 — the stack's root

## Affected Features

- [rust-code-restructuring.md](../../coder/rust-code-restructuring.md) — gains a cross-crate move
  operation; the `Operations` table and `Known limitations` both change
- [host-registry.md](../../../packages/tddy-daemon/docs/host-registry.md) — the registry moves to its
  own crate and its own gRPC service
- [host-tooling-probe.md](../../../packages/tddy-daemon/docs/host-tooling-probe.md) — same
- [host-add-key.md](../../../packages/tddy-daemon/docs/host-add-key.md) — same; the key-unlocking path
  moves with the host service rather than with auth
- [worktrees.md](../../../packages/tddy-daemon/docs/worktrees.md) — worktree listing, disk usage and
  file browsing move to their own crate and service
- [remote-git-service.md](../../../packages/tddy-daemon/docs/remote-git-service.md) — moves as-is
  (`remote_git.proto` is already its own service)
- [rpc-playground.md](../../daemon/rpc-playground.md) — the service picker gains entries and loses
  17 methods from `connection.ConnectionService`

## Summary

`tddy-daemon` is 82,546 source lines in 106 flat modules and is the largest package in the workspace.
Its wiring layer — the part that should be all that remains — is already only 2,715 lines. This PRD
covers the **root node** of the `#unbundle` stack, which does three things that together make the
other seven nodes possible, and proves them by carrying one subsystem group all the way through:

1. **Teaches `tddy-tools restructure` to move a module into another crate**, which it cannot do today
   at all.
2. **Extracts a shared `tddy-daemon-kernel`** holding the five symbols every subsystem reaches into
   `connection_service` for, and cuts the nine dependency cycles that would otherwise make any crate
   split impossible.
3. **Moves the host and worktree subsystems into `tddy-host-service` and `tddy-worktree-service`**,
   splitting families E, F, G and H — **17 of `ConnectionService`'s 90 methods** — into their own gRPC
   services, and migrating `packages/tddy-web` to the new coordinates.

## Background

`ConnectionService` is one gRPC service with **90 methods** and a 111-member Rust trait, implemented
by a single 60-field god object. The `docs/dev/todo/` backlog has recorded the problem three times
(2026-08-29 at 19,600 lines, 2026-09-06 at 22,800) and each entry explains why nobody acted: a split
would bury a reviewable feature under a mechanical move.

An intra-package split has since been done on this node's own base branch
(`feature/connection-service-split/lsp-settle-budget`, PR #468): 23,099 lines became a 2,416-line
facade over 60 modules. That branch's changeset names exactly where it had to stop —

> `impl ConnectionServiceTrait` cannot go under 500 lines, and this is arithmetic rather than a
> tooling limit. … Going lower means splitting `ConnectionService` into several gRPC services in the
> `.proto` — a protocol change, out of scope for a behaviour-preserving restructure.

**That protocol change is what this stack does**, and this node is where it starts.

### Why the tooling has to change first

`tddy-tools restructure` cannot move an item across a crate boundary, and this is by construction
rather than by omission: `move_symbol` and `move_file` are absent from the Rust backend's `SUPPORTED`
array, `RefactorOp::to` is read nowhere on the Rust path, `edit_for` emits exactly one
`FileEdit::Change` for the anchor's own file, `extract_module_to_file` names and places the file
itself beside its parent, and `rewrite_import_path` was **deliberately removed** from the operation
vocabulary — *"a vocabulary that advertises what cannot be performed is worse than a smaller one"*.
Nothing in the crate reads or writes a `Cargo.toml`, and there is no codemod anywhere in the repo.

Without a cross-crate operation, all eight nodes would hand-edit thousands of `use` paths. With one,
each node is a plan the tool executes and `restructure verify --against <ref>` proves.

## Proposed Changes

### What changes — 1. the restructure tooling

**A live defect fixed.** `rename_symbol` today takes the single-document path and `edits_for` filters
rust-analyzer's `documentChanges` down to the anchor's own URI. rust-analyzer *does* compute
cross-file rename edits; the backend **throws every other file's edits away**. A rename of a symbol
referenced elsewhere therefore breaks those callers silently. That is fixed here, and it is what makes
caller re-pointing possible at all.

**A new operation, `move_module_to_crate`.** Anchored on a module, it:

- emits `FileEdit::Rename` (which `apply.rs` already performs with `git mv`, so history survives) —
  today `convert_change` refuses every resource operation but `create`;
- rewrites the moved file's own `use crate::…` / `use super::…` header;
- re-points every caller, from a real `textDocument/references` result rather than a text search;
- edits both `Cargo.toml`s — the destination's `[dependencies]` and the workspace `members` list;
- optionally leaves `pub use <new_crate>::…;` in the source crate, so a move can have **zero caller
  diff** exactly as `reexport: "glob"` does one level down.

The transformation is **authored by the package but engine-informed**. That is a deliberate exception
to the "an engine performs it" rule, and it is the third such exception rather than the first —
`extract_class` and the facade `use` line already work this way. It is recorded as such in the crate's
own docs, with what it costs.

**A file-budget report.** `restructure check` gains a report of files over a line budget, so each of
the eight nodes can show mechanically which of the 87 files currently over 500 lines it brought under
budget and which it did not.

### What changes — 2. the daemon kernel

A new `tddy-daemon-kernel` crate takes the five symbols that every subsystem reaches into
`connection_service` for:

| Symbol | Reached by |
|---|---|
| `AgentActivityHub` | the sandbox subsystem (5 sites), `session_agent_inference.rs:36` |
| `now_unix_ms()` | the sandbox subsystem (2 sites), `telegram_session_subscriber.rs:122` |
| `HOST_DOCUMENT_FRAME_BYTES` | `context_files.rs:41` |
| `SessionUserResolver`, `SessionsBaseResolver` | `auth.rs:25` and five further subsystems |
| the nine-symbol spawn preamble | `cursor_cli_spawn.rs`, 7 sites — the worst single edge in the crate |

and nine dependency cycles are cut, because **Rust crates cannot be mutually dependent**. Two matter
beyond this node: `config.rs:85` reaches `session_room::DEFAULT_GIT_TIMEOUT`, which would make the
wiring layer depend on the LiveKit subsystem and vice versa; and `host_tooling ⇄ ssh_agent`, which is
what currently welds the host subsystem to the auth subsystem.

`server.rs::run_server`'s **12 positional arguments** become an options struct. This is not a tidiness
change: every one of the eight nodes removes a service from that list, and
`docs/dev/todo/2026-09-06-server-rs-run-server-takes-12-positional-arguments.md` already scoped the fix
to its own PR *because* it moves the `tddy-desktop` caller, which is outside the CI gate.

A **generated-code drift gate** is added to CI. Committed TypeScript in `packages/tddy-web/src/gen/`
is already stale and *nothing detects it*: regenerating `tddy-rust-typescript-tests/gen` produces a
5,182-line diff plus twelve never-committed files, and `codex_oauth_pb.ts` has no corresponding
`.proto` at all. Every later node regenerates TypeScript; without this gate their drift is
indistinguishable from pre-existing drift.

### What changes — 3. the host and worktree services

`connection.proto`'s single service block is cut for the first time. Families **E** (hosts: 7),
**F** (host telemetry: 1), **G** (worktree lifecycle: 6) and **H** (worktree files: 3) become:

| New service | Methods |
|---|---|
| `host.HostService` | `ListEligibleDaemons`, `ListKnownHosts`, `GetHostTooling`, `StreamHostPrompts`, `AnswerHostPrompt`, `AddHostKey`, `ListHostKeyCandidates`, `StreamHostStats` |
| `worktree.WorktreeService` | `ListWorktreesForProject`, `RemoveWorktree`, `StreamWorktreeStats`, `CalculateWorktreeSize`, `CleanWorktree`, `RestoreSessionWorktree`, `ListWorktreeDirectory`, `ReadWorktreeFile`, `StreamReadWorktreeFile` |

**Corrected during implementation.** The plan assumed this node would introduce the shared
`types.proto` every later node imports, on the strength of a planning-time list of ~25 cross-family
shared messages. Walking the field types of `connection.proto`'s 238 messages refutes that for *these*
families: the closure families E and F reach is 31 messages, G and H reach 20, and there is **zero
overlap between them and zero overlap with the closure of everything that stays**. `WorktreeRow`,
`ProbeOutcome` and `WorktreeSizeStatus` were all on that list and are reached only from inside the
moving set.

So this cut needs no shared types file, and the shared-types decision belongs to **node 6**, where
families I, J, R and S genuinely do share `SessionAttachment`, `StagedAttachmentRef` and
`HostDocumentRef`. A test pins the absence so a later node cannot re-couple the protos by importing
one out of habit.

The Rust source moves with the surface: the host subsystem (9 modules), the git/worktree subsystem
(8 modules), and the host-key path (`host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add`)
— which travels with the host service rather than with auth precisely because `AddHostKey` and
`ListHostKeyCandidates` are host-service methods, and because that is what cuts the
`host_tooling ⇄ ssh_agent` cycle.

### What stays the same

- **Every host and worktree behaviour.** This node changes where code lives and what coordinate
  addresses it; it changes no outcome. `restructure verify --against <ref>` proves the statement
  multiset is unchanged, and the moved-line diff proves each seam.
- **The remaining 73 `ConnectionService` methods**, until their own nodes.
- **The wire transports.** On every transport but the local UDS one, a new service is a new
  `ServiceEntry` and nothing more. The UDS/tonic path costs one hand-written adapter per service,
  because `tddy-codegen`'s `generate_tonic_adapter` is a documented stub.
- **`tddy-web`'s transport layer.** `useHttpClient(service)` and `clientFor<S extends DescService>(service)`
  are already service-generic; only import paths, call sites and six hard-coded bindings move.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-code-restructuring` | one defect fix, one new operation, one report; a new self-authored transformation to document |
| `tddy-daemon` | −7,312 prod LoC and −19 modules; `run_server`'s signature; 9 cycles cut |
| `tddy-service` | `connection.proto` loses 17 methods; `host.proto`, `worktree.proto` and `types.proto` appear; three `.connection.*` extern paths in the sandbox tonic pass must be re-pointed or the sandbox codegen breaks with a confusing message |
| `tddy-web` | regeneration plus the migration of every host and worktree call site; the 736-line Cypress `connectionServiceBackend.ts` fake splits |
| `tddy-desktop` | `run_server`'s caller moves. **Outside the CI gate** — must be built locally and said so |
| CI | one new drift-gate step; new crates are covered automatically, since clippy and nextest already run `--workspace` |

### User-facing

None intended. A daemon and a web bundle from this node talk to each other; **a web bundle from before
it does not**, because 17 method coordinates move. That is the licence this stack was given, and it
matches the repo's de-facto policy — *"break freely, migrate every consumer in the same change"*.

## Implementation Plan

1. Fix `edits_for`'s single-document filter; prove a rename re-points a caller in another file.
2. Add `move_module_to_crate` end to end, with the manifest edits and the crate-level facade.
3. Add the file-budget report to `restructure check`.
4. Extract `tddy-daemon-kernel`; cut the nine cycles; `run_server` options struct; the CI drift gate.
5. Cut `connection.proto` into `host.proto` + `worktree.proto`; wire `build.rs`, `lib.rs` and the
   descriptor set. No shared types file and no sandbox extern-path change — nothing those name moves
   in this node.
6. Move the host, worktree and host-key modules into their two new crates **using the operation from
   step 2** — the first real use, and the proof it works.
7. Migrate `tddy-web`, the daemon's own peer-forwarding literals, and the ~85 affected daemon tests.

## Acceptance Criteria

- [ ] `rename_symbol` re-points a caller in a different file; a rename that would strand a caller no
      longer silently does so
- [ ] `move_module_to_crate` moves a module to another crate, rewrites its header, re-points every
      caller found by `textDocument/references`, and edits both manifests
- [ ] `move_module_to_crate` with a crate-level facade produces **zero caller diff**
- [ ] `restructure verify --against <pre-move ref>` reports no lost or gained statements across the move
- [ ] `restructure check` reports which files exceed a given line budget
- [ ] `tddy-daemon` no longer holds `AgentActivityHub`, `now_unix_ms`, `HOST_DOCUMENT_FRAME_BYTES` or
      the two resolver aliases in `connection_service`
- [ ] `cargo build -p tddy-daemon` succeeds with **no** cycle between the wiring layer and any subsystem
- [ ] `run_server` takes one options struct; `tddy-desktop` builds (verified locally, not by CI)
- [ ] CI fails when committed generated TypeScript does not match a fresh generation
- [ ] `host.HostService` serves all 8 methods and `worktree.WorktreeService` all 9, over Connect-HTTP,
      LiveKit and the local UDS socket
- [ ] `connection.ConnectionService` no longer declares any of those 17 methods
- [ ] `tddy-web` reaches every host and worktree RPC at its new coordinate
- [ ] `./test -p tddy-daemon -p tddy-host-service -p tddy-worktree-service -p tddy-code-restructuring`
      matches the recorded baseline, with the one known pre-existing failure still at exactly one

## References

- Changeset: [2026-09-09-unbundle-host-worktree-services.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-host-worktree-services.md)
- Discovery: [2026-09-09-unbundle-host-worktree-services-initial-discovery.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-host-worktree-services-initial-discovery.md)
- Base branch's changeset: `docs/dev/1-WIP/2026-09-09-connection-service-split.md` (PR #468)
- [rust-code-restructuring.md](../../coder/rust-code-restructuring.md)
- [`docs/dev/todo/2026-08-29-connection-service-rs-is-19-600-lines.md`](../../../docs/dev/todo/2026-08-29-connection-service-rs-is-19-600-lines.md)
- [`docs/dev/todo/2026-09-06-server-rs-run-server-takes-12-positional-arguments.md`](../../../docs/dev/todo/2026-09-06-server-rs-run-server-takes-12-positional-arguments.md)
