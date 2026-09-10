# Changeset: cross-crate moves, the daemon kernel, and the host/worktree services

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **1 of 8** — the root. PR [#470](https://github.com/uppin/tddy-coder/pull/470).
Base: `master`. PR #468 (`feature/connection-service-split/lsp-settle-budget`) **merged on 2026-09-09**
(squash commit `ac002643`), and this node was repointed onto `master` the same day.

## Initial Discovery

Full codebase exploration that grounded this plan, and the whole `#unbundle` stack:
[2026-09-09-unbundle-host-worktree-services-initial-discovery.md](./2026-09-09-unbundle-host-worktree-services-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

This PR owns three things, in the order they must happen:

1. **A cross-crate move operation in `tddy-tools restructure`** — `move_module_to_crate`, plus the
   `edits_for` fix that makes caller re-pointing possible at all, plus a file-budget report.
2. **`tddy-daemon-kernel`** — the five symbols every subsystem reaches into `connection_service` for,
   the nine dependency cycles cut, `run_server`'s options struct, and a CI generated-code drift gate.
3. **`tddy-host-service` and `tddy-worktree-service`** — families E, F, G and H (17 of
   `ConnectionService`'s 90 methods) as `host.HostService` and `worktree.WorktreeService`, with their
   Rust source, their tests, and `tddy-web` migrated to the new coordinates.

**Correction from wave 2 — there is no `types.proto`, and there should not be.** The plan assumed
this node would introduce the shared types file every later node imports. Walking the field types of
`connection.proto`'s 238 messages says otherwise: the closure of messages families E, F, G and H
reach is **31 for hosts and 20 for worktrees, with zero overlap between them and zero overlap with
the closure of everything that stays**. `WorktreeRow`, `ProbeOutcome` and `WorktreeSizeStatus` were
all on the planning-time list of cross-family shared messages and are in fact reached only from
inside the moving set.

So node 1's cut needs no shared types file at all, and creating one here would mean moving messages
this node does not use so that a later node can — which is the stubs-as-deliverable shape the
boundary contract forbids. **The shared-types decision moves to node 6**, where families I, J, R and
S genuinely do share `SessionAttachment`, `StagedAttachmentRef` and `HostDocumentRef`. A test in
`packages/tddy-service/tests/unbundle_service_split.rs` pins the absence, so a later node cannot
re-couple the two protos by importing one out of habit.

**This node also de-duplicates, not only relocates.** Three of the five kernel symbols already
existed more than once, and two of them not identically:

| Symbol | Found | Behaviour |
|---|---|---|
| `now_unix_ms` | **three times** | `session_agent_status.rs:303` saturates at `u64::MAX`; `host_registry.rs:579` returns `i64` with an explicit pre-1970 refusal; `connection_service/host_messages.rs:270` uses a bare `as u64` that **truncates** |
| `SessionUserResolver` | **twice** | `task_service.rs:18` and `connection_service/service_util.rs:125`, identical shapes. Node 3 moves `task_service.rs` into `tddy-task`, so left alone this duplication would have become cross-crate |
| `trim_to_option` | twice inside `connection_service` | recorded in `docs/dev/todo/` |

Consolidating `now_unix_ms` is therefore a **behaviour decision, not a move**, and it is made here
once — see `## Decisions & Trade-offs`.

## Boundaries

This PR explicitly does **not**:

- Move any subsystem other than hosts, git/worktrees and the host-key path. Model registry, telegram,
  screen sharing, sandbox, spawn, auth, LiveKit and the leaf RPC services are nodes 2–4.
- Touch `packages/tddy-tools`' own module layout beyond `restructure_cli.rs`. Thinning `tddy-tools`
  is node 5.
- Split the remaining 73 `ConnectionService` methods. Families B, I, J, K, L, M, N, P, R, S and T
  belong to nodes 4 and 6–8; families C, D, O and Q stay in the daemon deliberately.
- Add a TypeScript **workspace package** per generated service. The generated TS stays in
  `packages/tddy-web/src/gen/`, so bun resolution, `bun.lock`, `local.bun.lock` and `.local-install/`
  are untouched.
- Retire `packages/tddy-terminal-rpc/proto/terminal_session.proto`. That is node 6's job.
- Force every file under 500 lines. Per the agreed policy the seams are cut where they are cohesive,
  and whatever stays over budget is recorded below rather than split to hit a number.

## Dependencies

**None — this is the stack's root, and it now bases on `master`.** It was cut on
`feature/connection-service-split/lsp-settle-budget` (PR #468), which is not a `#unbundle` node but
supplies two things this PR requires. **PR #468 landed in `master` on 2026-09-09** (squash commit
`ac002643`), so both are now in the trunk:

| From the base branch | What it delivers | How this PR consumes it |
|---|---|---|
| the four `tddy-lsp` bridge fixes (`28fa278d`, `2afaec79`, `0af927b4`, `3d264466`) | JSON-RPC errors reach the caller; `ContentModified`/timeout classify as `ServerCatchingUp`; `--indexing-budget` drives the request timeout; the bridge client advertises `codeAction` support and utf-8 positions | **without them `restructure apply` fails on every operation** with a message that reads like a plan defect. This is why the stack is based here and not on `master` |
| the intra-package split of `connection_service.rs` | a 2,416-line facade over 60 modules under `connection_service/` | the host and worktree handler bodies are already in named modules, so this PR moves modules rather than carving a 23,000-line file |

✅ **Repoint done.** #468 merged on 2026-09-09 and this PR was repointed onto `master` the same day:
its two commits were restacked with `git rebase --onto origin/master e6fd2213`, and the `#unbundle`
stack was re-registered on GitHub with `master` as its trunk (stack #479). Successor nodes 2–8 keep
their existing bases and each need their own `/pr-stack-rebase`.

## Draft PR contract

What lands in this PR's **second commit**, to unblock nodes 2–8 as early as possible:

- `RefactorKind::MoveModuleToCrate` in `packages/tddy-code-restructuring/src/plan.rs`, its wire schema
  fields, and its entry in `SUPPORTED` — the signature nodes 2–8 write plans against.
- The `move_module_to_crate` public entry point in the Rust backend, with `// TODO(host-worktree-services): implement`
  bodies, building and `clippy -D warnings` clean so dependents compile.
- `packages/tddy-daemon-kernel/src/lib.rs` declaring the five moved symbols with their real
  signatures — the surface nodes 2, 3 and 4 import.
- `packages/tddy-service/proto/host.proto` (8 rpcs, 31 messages) and `worktree.proto` (9 rpcs, 20
  messages), generated by two new `build.rs` passes and exported as
  `tddy_service::{HostServiceServer, WorktreeServiceServer}`. **No `types.proto`** — see the
  correction in `## Responsibility`.
- The failing acceptance and unit tests for all of the above.

**Landed in commit 2** (`cargo build --workspace`, `cargo fmt --all --check` and
`cargo clippy -p tddy-code-restructuring -p tddy-daemon-kernel -p tddy-service --all-targets -- -D warnings`
all clean, so nodes 2–8 compile against it):

| Surface | Where |
|---|---|
| `RefactorKind::MoveModuleToCrate`, its `to`/`reexport` validation, `SUPPORTED` 7 → 8 | `tddy-code-restructuring/src/plan.rs`, `backends/rust.rs` |
| `Destination`, `CallerRewrite`, `Survey`, `survey`, `resolve`, `facade_line` | `tddy-code-restructuring/src/crate_move.rs` (new) |
| `ModuleReferences`, `ItemReferences`, `Reference` — the engine seam `survey`/`resolve` take (green phase; see `## Decisions & Trade-offs`) | `tddy-code-restructuring/src/crate_move.rs` |
| `workspace_edits_for` — the multi-document primitive the rename fix and the caller re-pointing both need | `backends/rust.rs`; **wired into `rename_symbol` in the green phase**, so the `#[allow(dead_code)]` and its TODO are gone |
| the five kernel symbols plus `trim_to_option` | `tddy-daemon-kernel/src/lib.rs` (new crate, workspace member) |
| `host.HostService`, `worktree.WorktreeService` | `tddy-service/proto/`, `build.rs`, `src/lib.rs` |

**19 failing tests** define the behaviour: 9 in `tddy-code-restructuring` (3 pinning the cross-file
rename defect, 6 on the move itself), 8 in `tddy-daemon-kernel`, 2 in `tddy-service` (the
completion criterion — the 17 methods are still declared on `connection.ConnectionService`).

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — nothing in the stack precedes it. Its own tests need only the base
branch's tree plus the surface it declares itself.
**Concurrent with:** nothing. It is the root, and every other node consumes something it delivers.
**Blocks:** all seven other nodes — nodes 2–5 need `move_module_to_crate` and the kernel; nodes 6–8
need those plus the shared `types.proto` pattern.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n5 → n6, n7, n8

Note that once this node is green, **nodes 2, 3, 4 and 5 are all greenable at once** — they touch
disjoint subsystems, disjoint protos and disjoint test files. Nodes 6, 7 and 8 wait on node 5 because
it moves the `tddy-service`, `tddy-terminal-rpc` and `tddy-tool-engine` surfaces they then serve from.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../packages/tddy-code-restructuring/README.md) — a new
  operation, a defect fix, a report
- **tddy-tools**: `src/restructure_cli.rs` only — the new operation's CLI surface
- **tddy-daemon-kernel** *(new)* — the shared symbols
- **tddy-host-service** *(new)* — the host subsystem and `host.HostService`
- **tddy-worktree-service** *(new)* — the git/worktree subsystem and `worktree.WorktreeService`
- **tddy-daemon**: `packages/tddy-daemon/` — 21 modules leave for the two services, plus `config` to the kernel; `run_server`'s signature; nine cycles cut
  - [host-registry.md](../../packages/tddy-daemon/docs/host-registry.md), [host-tooling-probe.md](../../packages/tddy-daemon/docs/host-tooling-probe.md),
    [host-add-key.md](../../packages/tddy-daemon/docs/host-add-key.md), [worktrees.md](../../packages/tddy-daemon/docs/worktrees.md),
    [remote-git-service.md](../../packages/tddy-daemon/docs/remote-git-service.md) — move to the new packages' docs
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — 931 lines, loses 17 endpoint entries
- **tddy-service**: `connection.proto` shrinks; `types.proto`, `host.proto`, `worktree.proto` appear;
  `build.rs` gains passes and its sandbox extern paths are re-pointed; `src/lib.rs` gains modules
- **tddy-web**: every host and worktree call site, six hard-coded `ConnectionService` bindings, the
  regenerated `src/gen/`, and the Cypress fakes
- **tddy-desktop**: **not affected after all.** The plan recorded `src-tauri/src/lib.rs` as "the
  `run_server` caller"; it is not one. The package's only occurrence of `run_server` is a prose
  reference inside a `TODO` comment at `lib.rs:257` — the desktop host builds its own
  `MultiRpcService` and serves over Tauri IPC with no HTTP listener. It was still built and linted
  locally (it is **outside the CI gate**) to prove the options struct did not reach it: both clean

## Related Feature Documentation

- [PRD-2026-09-09-host-worktree-services.md](../../ft/daemon/1-WIP/PRD-2026-09-09-host-worktree-services.md)
- [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) — its `Operations` table and
  `Known limitations` both change

## Successor PRs

Forward links only. Later nodes must not link back here, because this document leaves `1-WIP` first.

- `feature/unbundle/model-telegram-screen` — node 2
- `feature/unbundle/sandbox-spawn-services` — node 3
- `feature/unbundle/auth-livekit` — node 4
- `feature/unbundle/tools-thinning` — node 5
- `feature/unbundle/session-io-services` — node 6
- `feature/unbundle/session-agent-services` — node 7
- `feature/unbundle/exec-prstack-services` — node 8

## Summary

`tddy-tools restructure` learns to move a module into another crate; `tddy-daemon` gives up the five
symbols and nine cycles that pin its subsystems together; and the host and worktree subsystems become
two crates serving two new gRPC services, with 17 methods leaving `connection.ConnectionService` and
`tddy-web` migrated to their new coordinates.

## Background

See the PRD for the full argument. In brief: the daemon's wiring layer is already only 2,715 prod LoC,
so the target state is reached by removing rather than writing — but nothing can be removed mechanically
today, because the restructure tool cannot cross a crate boundary and nine cycles bind the subsystems
to each other and to the wiring layer.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `2026-09-06-server-rs-run-server-takes-12-positional-arguments.md`

Every one of the eight nodes removes a service from `run_server`'s argument list. Left alone, each
node either appends a thirteenth positional argument or renumbers the existing twelve, and the entry
already records why that cannot be deferred to a later node: *"An options struct is the right fix, but
it moves `main.rs` and the desktop caller, and `tddy-desktop` is outside the CI gate — so it wants its
own PR."* It gets that PR here, at the root, and earns a `## Scope` line.

### ⛔ BLOCKING — `2026-08-14-tddy-rust-typescript-tests-gen-is-badly-stale-and-nothing-detects-it.md` and `2026-09-06-packages-tddy-web-src-gen-daemon-config-pb-ts-was-regenerated-without.md`

Committed generated TypeScript is already stale and nothing detects it — regenerating
`tddy-rust-typescript-tests/gen` produces a 5,182-line diff plus twelve never-committed files, and
`packages/tddy-web/src/gen/codex_oauth_pb.ts` has no corresponding `.proto`. Every node from here on
regenerates TypeScript. Without a gate, each node's own drift is indistinguishable from what was
already broken, and a reviewer cannot tell a mistake from inherited rot. Earns a `## Scope` line.

### ℹ ANSWERED — `2026-08-29-connection-service-rs-is-19-600-lines.md` and `2026-09-06-connection-service-rs-is-22800-lines.md`

Both entries ask for the cohesive groups a split would follow and name the cost. The 2026-08-29 entry
predicted it exactly: *"`ConnectionServiceImpl`'s ~60 private fields would have to become `pub(crate)`
or move behind accessors."* Discovery confirms the prediction and sharpens it — **`pub(crate)` does
not cross a crate boundary**, so each subsystem's state must move behind an owned struct or a trait,
which is what the 30 existing `pub trait` ports and the 21 trait-injecting `with_*` builders make
affordable. The 2026-09-06 entry names *"the host registry, the tooling probe and the prompt
handlers"* as a further cohesive seam not among the ones it listed. **That seam is this node.** Both
entries are answered by this stack and should be closed at wrap.

### ⚠ DURING — `2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`

`tddy-desktop` is a second embedder of the daemon (*"the desktop app is `tddy-daemon` in one
process"*) and consumes exactly the wiring layer this node reshapes —
`{config, runtime, supervisor_client, spawn_worker, cli_session_manager}`. It is **outside the CI
gate**, so this node can break it with green checks. The desktop build is verified locally and the
result stated explicitly; it is not claimed on CI's authority.

### ⚠ DURING — `2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md`

A duplicated trim-to-`Option<String>` helper inside the file being carved up. Each node that carries
handlers out must take it to **one** home rather than copying it per crate. Here that home is
`tddy-daemon-kernel`.

## Scope

- [x] **Tooling — cross-file edits**: `edits_for` stops discarding other documents' rename edits ✅
- [x] **Tooling — `move_module_to_crate`**: the operation, its schema, manifest edits, crate-level facade ✅ — **13 of 21 modules moved by the operation** on the real tree, in two dependency layers; the 8 it cannot take are recorded in `## Technical Debt`
- [x] **Tooling — file-budget report**: `restructure check --budget LINES` reports the files a plan's own anchors name that exceed it ✅ — a **report, not a gate**, per the best-effort decision below
- [x] **Prerequisite — `run_server` options struct** (⛔ blocking TODO) ✅ — **there is no desktop caller**, see the correction below
- [x] **Prerequisite — generated-code drift gate** in CI (⛔ blocking TODO) ✅ `scripts/generated-code.sh`, green on all four generated directories
- [x] **Kernel**: `tddy-daemon-kernel` with the five shared symbols; the trim helper's one home ✅
- [~] **Cycles**: audited against the tree rather than the discovery table — **3 of the 9 listed do not exist**, 3 are cut here, 3 are classification cuts that land with the crate moves. See `## Decisions & Trade-offs`
- [x] **Proto**: `host.proto` + `worktree.proto` declared and generating; `connection.proto` **90 → 73 rpcs**, 51 messages/enums removed; **no `types.proto` needed**; sandbox extern paths untouched ✅
- [x] **Crates**: `tddy-host-service` (13 modules + `host_messages` + 5 handler-test modules + 1 integration suite), `tddy-worktree-service` (8 modules + 7 integration suites) ✅
- [x] **Web**: regeneration ✅ — `connection_pb.ts` and `tddy-rust-typescript-tests/gen/connection_pb.ts` shrink by 4,126 lines; `scripts/generated-code.sh check` exits 0. Call-site migration, the six bindings and the Cypress fakes landed with M8
- [x] **File budget**: recorded ✅ — a report, not a gate, per the best-effort decision below.
      **Twelve of this node's files stand over 500 lines.** Eleven of them *moved* rather than
      being written here; the twelfth, `crate_move.rs`, this node wrote. It is absent from the
      `restructure check --budget` run above for a mechanical reason worth stating: that command
      measures only the files a plan's **anchors** name, and `crate_move.rs` is the operation's own
      implementation, never a plan anchor. So the tool cannot see its own size, and the record has to
      say so rather than let the omission read as a clean result:

      | File | Lines | Why it stands |
      |---|---|---|
      | `tddy-code-restructuring/src/crate_move.rs` | 1,609 (1,023 prod + 586 tests) | **written by this node**, not moved. Well decomposed — 40 functions, only `planned` over 40 lines — and a split into `manifest.rs` + `header.rs` is available and cheap, but buys little while the operation is still settling. Recorded rather than split, per the policy above |
| `tddy-daemon-kernel/src/config.rs` | 2,448 | `DaemonConfig` and its nested blocks — one serde schema, and splitting a config struct splits nothing cohesive |
      | `tddy-host-service/src/host_add_key_handler_tests.rs` | 1,259 | 26 tests of one flow (prompt → encrypt → decrypt → unlock → agent); moved verbatim |
      | `tddy-host-service/src/host_registry.rs` | 1,075 | moved verbatim |
      | `tddy-host-service/src/host_tooling.rs` | 1,074 | moved verbatim |
      | `tddy-worktree-service/src/worktrees.rs` | 1,052 | moved verbatim |
      | `tddy-host-service/src/host_private_key.rs` | 995 | moved verbatim |
      | `tddy-worktree-service/src/remote_git_service.rs` | 864 | moved verbatim |
      | `tddy-host-service/src/ssh_agent.rs` | 799 | moved verbatim |
      | `tddy-worktree-service/src/project_storage.rs` | 653 | moved verbatim |
      | `tddy-worktree-service/src/worktree_files.rs` | 629 | moved verbatim |
      | `tddy-host-service/src/service.rs` | 880 | the 8 handlers, the state, the builders and the peer routing — authored here, and cut at the service boundary the proto draws |

      The two files this node authored and could have split are `host_service.rs`'s service (880)
      and `worktree_service.rs`'s (753). Both are one service's handlers plus the state they read;
      splitting them would separate a handler from the field it consults, which is the cohesion the
      budget exists to protect. **`tddy-daemon` shrank**: `connection_service/rpc_service.rs` 6,938
      → 5,712 and `connection_tonic_adapter.rs` 1,505 → 1,306, and 21 modules left the crate
- [x] **Baseline**: recorded per touched package, with a ledger accounting for every remaining failure ✅
- [x] **Code Quality**: `cargo clippy -p <each> --all-targets -- -D warnings` clean, `cargo fmt --all --check` clean ✅
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | |
|---|---|
| `tddy-code-restructuring` | 7 Rust operations; `MoveSymbol`/`MoveFile` refused with `UnsupportedOp`; `RefactorOp::to` unread on the Rust path; `edit_for` writes one file; `convert_change` refuses every resource op but `create`; `edits_for` discards non-anchor documents' rename edits; nothing reads a `Cargo.toml` |
| `tddy-daemon` | 106 flat modules, 82,546 src LoC. `connection_service.rs` a 2,416-line facade over 60 modules, one of which (`rpc_service.rs`) is 6,278 lines holding all 90 trait-method bodies inline. Wiring layer 2,715 prod LoC. `run_server` takes 12 positional args under `#[allow(clippy::too_many_arguments)]` |
| coupling | 8 files outside `connection_service` reference it in code, 4 of them one symbol each; 9 dependency cycles; 30 `pub trait` ports and 21 trait-injecting builders already exist |
| `connection.proto` | 2,953 lines, one service, **90 methods** (69 unary, 21 server-streaming, 1 bidi), 223 messages, 15 enums, bare `package connection;` |
| `tddy-web` | 244 `gen/connection_pb` imports (116 in 112 `src/` files, 130 in 126 `cypress/` files); 86 call sites in 45 `src/` files; 6 hard-coded `ConnectionService` bindings in `src/rpc/`; a 736-line Cypress fake |
| CI | clippy and nextest run `--workspace`; **no** step regenerates or diffs generated code |

### State B

| | |
|---|---|
| `tddy-code-restructuring` | 8 Rust operations, `move_module_to_crate` among them; rename re-points callers; `check` reports a file budget; the crate's docs record a third self-authored transformation |
| `tddy-daemon-kernel` | `AgentActivityHub`, `now_unix_ms`, `HOST_DOCUMENT_FRAME_BYTES`, `SessionUserResolver`, `SessionsBaseResolver`, the spawn preamble, the trim helper |
| `tddy-host-service` | 9 host modules + the 4 host-key modules; serves `host.HostService` (8 methods) |
| `tddy-worktree-service` | 8 git/worktree modules; serves `worktree.WorktreeService` (9 methods) |
| `tddy-daemon` | 87 modules; no cycles; `run_server(RunServerOptions)`; registers two more `ServiceEntry`s |
| `connection.proto` | 73 methods; **no `types.proto`** — the closure of the moving families shares nothing with what stays, so `host.proto` and `worktree.proto` import nothing. See the correction in `## Responsibility` |
| CI | a step that regenerates and diffs committed generated code |

### Delta

#### tddy-code-restructuring
- **API**: `RefactorKind::MoveModuleToCrate`; `RefactorOp` gains the destination-crate fields; a new `crate_move.rs`
- **Fix**: `edits_for` keeps every document rust-analyzer returns
- **Implementation**: `convert_change` honours `rename`; `apply.rs`'s existing `git mv` path is reached for the first time from Rust; `Cargo.toml` read/write

#### tddy-daemon
- **Architecture**: the five shared symbols leave, six cycles are cut; 21 modules leave
- **API**: `run_server` takes an options struct
- **Implementation**: two new `ServiceEntry`s; two new hand-written tonic adapters for the UDS path

#### tddy-service
- **Proto**: `host.proto`, `worktree.proto` (**no `types.proto`**); `connection.proto` loses 17 rpcs and the 51 messages/enums that move
- **Build**: one prost + one tonic pass per new service; both added to the descriptor set; **the three `.connection.*` extern paths in the sandbox tonic pass re-pointed** — get this wrong and the sandbox codegen fails with a message that names neither cause

#### tddy-web
- **Implementation**: regenerate; migrate every host and worktree call site and the six hard-coded bindings; split the Cypress fake per service

#### tddy-desktop
- **Integration**: the `run_server` caller. Built locally; CI does not cover it

## Implementation Milestones

- [x] M1 — `edits_for` fixed; a rename re-points a caller in another file ✅
- [x] M2 — `move_module_to_crate` moves a module, rewrites its header, re-points callers, edits both manifests ✅ — engine half covered by the live-rust-analyzer suites, and proven in anger: it performed 10 of the 21 module moves in M7
- [x] M3 — the crate-level facade produces a zero-caller-diff move ✅ — settled by a compiler, not by reading the diff
- [x] M4 — file-budget report ✅; `run_server` options struct ✅; CI drift gate ✅ (green on all four generated directories)
- [~] M5 — `tddy-daemon-kernel` adopted by `tddy-daemon`, every duplicate deleted; `cargo build -p tddy-daemon` clean; cycles audited (see `## Decisions & Trade-offs`) rather than all nine cut, because three of the nine are not in the tree
- [x] M6 — `host.proto` + `worktree.proto` generate (**no `types.proto`** — see `## Responsibility`); sandbox extern paths untouched, because nothing they name moves in this node ✅
- [x] M7 — both crates exist and serve their methods on all three transports ✅ — two `ServiceEntry`s in `runtime.rs` (HTTP `/rpc` + the LiveKit common room) plus two hand-written tonic adapters on the local socket
- [x] M8 — `tddy-web` migrated ✅; Cypress component suites green — **231 specs, 1419/1419, 5m34s**
- [x] M9 — baselines restored ✅ and the file-budget outcome recorded ✅ — see `## Baseline` and its failure ledger

## Testing Plan

**Primary test level: integration**, per package, because every deliverable here is a boundary change
rather than an algorithm. Three distinct kinds:

- **The tooling** is tested against a real rust-analyzer, as the existing `tddy-code-restructuring`
  suites are — a fixture workspace with two crates, a module moved between them, and the callers
  asserted to resolve. These suites are load-sensitive and run `--test-threads=1`.
- **The move itself** is proven by `restructure verify --against <ref>` plus the moved-line diff, not
  by a test. Normalise `pub(crate)` and whitespace away, set-compare every moved line against the
  pre-move ref, and state how many lines differ and why each one does.
- **The services** are tested where their handlers' tests already live: the ~85 daemon test files that
  reach the host and worktree RPCs move to the new crates with the code, and drive the new service
  through its `ServiceEntry` rather than through `ConnectionServiceImpl`.

`tddy-web` keeps `mountWithRpc` + `anInMemoryRpcBackend`; `cy.intercept` is not used.

## Acceptance Tests

### tddy-code-restructuring
- [x] **Integration**: a rename of a symbol referenced from another file rewrites that file too (`rename_cross_file_acceptance.rs`) ✅
- [x] **Integration**: `move_module_to_crate` relocates a module, its header resolves, both manifests updated (`move_module_to_crate_acceptance.rs`) ✅
- [x] **Integration**: the same move with a crate-level facade leaves every caller untouched (`move_module_to_crate_acceptance.rs`) ✅
- [x] **Unit**: a plan naming `move_module_to_crate` without a destination crate is `MalformedPlan` (`plan.rs::refuses_a_cross_crate_move_with_no_destination`) ✅ — and a **named** facade is refused too, since it cannot serve a module move (`refuses_a_named_facade_on_a_cross_crate_move`)
- [x] **Unit**: `check` lists exactly the files over a given budget (`runner.rs`) ✅

### tddy-daemon
- [x] **Integration**: `run_server` accepts an options struct and serves the same bundle and routes (`server_options_acceptance.rs`) ✅ 6 tests
- [x] **Unit**: the kernel's five symbols resolve from a crate that does not depend on `tddy-daemon` (`tddy-daemon-kernel/tests/kernel_surface_acceptance.rs`) ✅ 10 tests

### tddy-host-service
- [~] **Integration**: all 8 `host.HostService` methods answer over Connect-HTTP — **delivered under different filenames.** The planned `host_service_acceptance.rs` / `host_add_key_acceptance.rs` / `host_stream_acceptance.rs` were not written; the equivalent coverage *travelled with the code* as the moved daemon suites plus the in-crate handler-test modules (`host_add_key_handler_tests.rs`, `host_stats_handler_unit_tests.rs`, `host_tooling_handler_unit_tests.rs`, `known_hosts_handler_unit_tests.rs`, `ssh_agent_block_handler_tests.rs`), which cover every one of the 8 methods. Recording the substitution rather than ticking a filename that does not exist
- [ ] **Integration**: `AddHostKey` unlocks and loads a key, with the host-key path in this crate (`host_add_key_acceptance.rs`)
- [ ] **Integration**: `StreamHostPrompts` and `StreamHostStats` stream and terminate cleanly (`host_stream_acceptance.rs`)

### tddy-worktree-service
- [~] **Integration**: all 9 `worktree.WorktreeService` methods answer over Connect-HTTP — **delivered under different filenames**, as the moved `worktrees_rpc.rs`, `worktree_files_rpc.rs`, `stream_worktree_stats_rpc.rs`, `stream_read_worktree_file_rpc_acceptance.rs` and `worktree_session_actions_acceptance.rs` suites. Same substitution as the host row above
- [ ] **Integration**: `StreamReadWorktreeFile` frames a file identically to the old coordinate (`worktree_file_frames_acceptance.rs`)

### tddy-web
- [x] **Cypress component**: the hosts screen loads through `host.HostService` (`HostsScreen.cy.tsx`) ✅
- [x] **Cypress component**: the worktrees screen loads through `worktree.WorktreeService` (`WorktreesAppPage.cy.tsx`) ✅ — pins the three-way split: worktree feed under `WorktreeService`, roster under `HostService`, `ListProjects` still under `ConnectionService`

### CI
- [x] **Integration**: the drift gate fails on a deliberately stale committed `*_pb.ts` and passes on a fresh one ✅ (`scripts/generated-code.test.ts`, 5 tests, runs in CI before the gate itself)

## Decisions & Trade-offs

- **The new operation is authored by this package, not delegated to an engine.** `plan.rs:40` records
  the rule it bends — *"`rewrite_import_path` [was] dropped for having no engine behind [it]; a
  vocabulary that advertises what cannot be performed is worse than a smaller one"*. rust-analyzer has
  no cross-crate move assist, so there is nothing to delegate to. The mitigation is that every caller
  rewritten comes from a real `textDocument/references` result rather than a text search, making it
  engine-*informed*; and this is the **third** self-authored transformation in the crate, after
  `extract_class` and the facade `use` line, not the first.
- **The host-key path travels with the host service, not with auth.** `AddHostKey` and
  `ListHostKeyCandidates` are host-service methods, and `host_tooling ⇄ ssh_agent` is one of the nine
  cycles. Moving `host_keypair`, `host_private_key`, `ssh_agent` and `ssh_agent_add` here cuts that
  cycle as a side effect of a move that was going to happen anyway. The cost is that node 4's auth
  crate is smaller than "auth" suggests, and its changeset says so.
- **Three deliverables in one PR.** The tooling, the kernel and the first service group would be three
  nodes under a strict reading of the boundary contract. They are one because each is worthless
  without the next in this stack — an unused operation, a kernel nothing imports, or a service group
  that had to be hand-migrated. Landing them together means the first real use of the operation is
  its own proof. The cost is a large diff, mitigated by the moved-line diff and `verify --against`,
  which is what makes a mechanical move reviewable without reading every line.
- **Generated TypeScript stays in `packages/tddy-web/src/gen/`.** A workspace package per generated
  service would need a root `workspaces` entry, a `main`/`types` pointing at source, a `workspace:*`
  entry, a fresh `bun install`, and regeneration of `local.bun.lock` and `.local-install/` — for no
  gain, since `buf generate` already runs over the whole proto directory and produces one `*_pb.ts`
  per proto with no config change.
- **`connection.proto` keeps its bare `package connection;`.** Introducing versioned package names is
  the one cheap opportunity this split offers and it is deliberately declined: it would put a second
  breaking change in every node's web migration. Recorded in `docs/dev/todo/` instead.
- **`now_unix_ms` saturates, and this is a behaviour change made deliberately.** Of the three
  implementations it replaces, one saturated at `u64::MAX`, one returned `i64` with an explicit
  pre-1970 refusal, and one used a bare `as u64` cast that **truncates**. A truncating cast is the
  wrong behaviour for a timestamp: it turns a clock far in the future into a timestamp in the past,
  silently, and downstream that is indistinguishable from correct data. Saturation is chosen over the
  `i64` refusal because every caller here stamps a record it is about to write, and a caller that
  cannot proceed without a plausible clock is better served by checking the clock than by receiving
  an error from a timestamp function. Callers that need the pre-1970 diagnostic keep their own check.
- **`workspace_edits_for` was published unwired, and is now wired.** At planning: It is the
  multi-document primitive both the rename fix and the caller re-pointing need, and nodes 2–8 compile
  against its signature. It is not yet wired into `rename_symbol` because `edits_for` still routes
  every single-document assist, and swapping it now would panic the 251 tests already passing through
  that path. The alternative — deleting it until the green phase — would leave nodes 2–8 without the
  signature this node exists to publish.
- **`survey` and `resolve` take an engine seam, not the backend.** Their red-phase signature
  `(&Workspace, &RefactorOp)` could not reach `textDocument/references` at all — `Workspace` carries
  only a root and an overlay — so `callers` could only ever have come back empty. Taking
  `&mut RustBackend` instead would have made `crate_move` and `backends/rust.rs` mutually dependent
  (the backend now imports `crate_move` to route the op), and making them `RustBackend` methods would
  put a language-agnostic transformation — a `git mv`, two manifest edits, a `pub use` line — inside
  the Rust engine driver and make it untestable without a live server. The `ModuleReferences` trait
  is the one row of this module's own decision table that only a server can answer, so that is where
  the seam is cut.
- **The nine cycles were audited against the tree, and the table was wrong about three of them.**
  Re-deriving the module graph found **15** mutual pairs at `ac002643`, not nine, and three the table
  names are not cycles at all: `livekit_peer_discovery → common_room_supervisor` is 0 (the arrows all
  run *out* of `common_room_supervisor`), `worktree_files → context_files` is 0, and
  `session_agent_status → session_agent_inference` is 0. What the last row was really describing is
  `connection_service ⇄ session_agent_inference`, which the `AgentActivityHub` lift does cut — along
  with `connection_service ⇄ sandbox_session`, `⇄ telegram_session_subscriber` and `⇄ context_files`,
  three cycles the table missed entirely and which the kernel cuts for free. Adopting the kernel plus
  inlining `DEFAULT_SESSION_ROOM_GIT_TIMEOUT` cuts **6** real cycles. The four that remain and are
  named in the table — `host_tooling ⇄ ssh_agent`, `host_tooling ⇄ remote_desktop_probe`,
  `livekit_peer_discovery ⇄ multi_host`, `telegram_notifier ⇄ telegram_session_control` — are
  *classification* cuts, not edits: each pair lands in the **same** destination crate, so the crate
  graph is acyclic the moment the modules move and forcing a module-level cut now would be churn with
  no boundary behind it — as is `telegram_multi_select_shortcuts ⇄ telegram_notifier`, a tenth pair
  the table did not name. Four further real cycles the table missed —
  `host_registry ⇄ livekit_peer_discovery`, `livekit_peer_discovery ⇄ split_session`,
  `connection_service ⇄ cursor_cli_spawn` (the spawn preamble the kernel does **not** carry) and
  `connection_service ⇄ test_util` — **do** span destination crates and are not cut here; they belong
  to the nodes that move those modules. The tree goes from **15** mutual module pairs to **9**.
- **A caller's rewritten path keeps the module segment**: `crate::host_registry::HostRegistry`
  becomes `tddy_host_service::host_registry::HostRegistry`, not `tddy_host_service::HostRegistry`.
  The module keeps its name in the crate it arrives in, which is what makes the glob facade free —
  `pub use tddy_host_service::*;` re-exports the module, so `crate::host_registry::X` keeps resolving
  in the crate it left. Re-exporting items at the destination root instead would need an authored
  `pub use` per move and a second rewrite rule.
- **The moved file's header pass is mechanical, and deliberately not `extract_module`'s.** That
  operation can ask the server which names went unresolved, because the items stay in a file it holds
  open; a module that has left its crate cannot be typed until it is *in* the destination, so there is
  no equivalent answer to ask for. What is provable without the server is that a `crate::`/`super::`
  qualifier at the head of a `use` declaration changed meaning by definition when the file changed
  crates, so those are re-pointed and nothing else is.
- **A facade that would make the workspace cyclic is refused up front.** A facade makes the origin
  depend on the destination; if the moved code still names the origin, the destination depends back,
  and cargo rejects the pair with an error naming neither the module nor the operation. The refusal
  names every path that forced it.
- **A crate that goes on naming the moved module gains a dependency on the crate it moved to.**
  Found by the live-rust-analyzer acceptance suite on its first run, not by review: the operation
  emitted a tree that read correctly and did not compile (`E0433`/`E0432`), on **both** paths — the
  facade names the destination, and so does every re-pointed caller. As written it would have broken
  the build on all 19 daemon modules, and all 271 unit tests passed through it, because a manifest
  that is never compiled looks fine. This is why every acceptance test here ends in `cargo check`:
  only a compiler tells an edit that looks right from one that resolves.
- **The dependency-cycle refusal covers the no-facade path too.** It originally fired only for
  facades; once a re-pointed caller also creates origin → destination, the no-facade path can close
  the same cycle. The condition is now "the destination would name the origin **and** the origin goes
  on naming the destination". That second defect was hidden by the first.
- **The drift gate's inherited rot was settled, not excluded.** It was red the day it was added, on
  drift no PR introduced. Rather than exclude a directory to get a green check, the stale directories
  were regenerated through the script's own `write` mode and the two `codex_oauth_pb.ts` orphans — no
  generating `.proto`, no TypeScript importer — deleted. The gate's first run found
  `tddy-web/src/gen/sandbox_pb.ts` genuinely stale: `sandbox.proto` had gained
  `in_jail_tool_request`/`in_jail_tool_response` and the committed TypeScript predated them.
- **`run_server`'s options struct has no `Default`.** A defaulted `host: ""` would fail to bind at
  runtime instead of failing to compile, so every caller states all twelve fields. Four call sites
  migrated — `main.rs` and three in the daemon's own tests; the desktop, which the plan expected to
  be one, is not.
- **`config.rs` moves into `tddy-daemon-kernel` whole, and that widens the kernel's charter beyond
  "the five shared symbols".** This is a deliberate decision, not drift, and it is the *only*
  whole-module move the kernel takes. Four of the moving modules (`host_registry`, `host_tooling`,
  `remote_desktop_probe`, `multi_host`) and every handler in both new services take
  `&DaemonConfig` and read disjoint parts of it, so `pub(crate)`-widening buys nothing and there is
  no smaller cut: the *symbol* is the file. The alternative — a narrow value struct per consuming
  crate — is authoring rather than moving, and nodes 2–8 would each have to repeat it, so the
  collision it is meant to avoid is exactly the one it would cause. `## Boundaries` reserves
  *subsystems* (model registry, telegram, screen sharing, sandbox, spawn, auth, LiveKit) for later
  nodes; `DaemonConfig` is not one. The daemon keeps `pub use tddy_daemon_kernel::config;`, so no
  caller in it changed.
- **Everything else the kernel gained is a symbol lift, measured before it was made.** The
  transitive call closure decided each boundary, not the file: `spawn_as_user` is **179 of
  `spawner.rs`'s 2,539 lines** (9 functions — `find_program_on_spawn_child_path`,
  `run_capture_as_user`, `start_output_as_user` and what they reach); `privilege_drop` is **63 of
  `pty_runtime.rs`'s 400** (5 symbols); `user_paths` is **34 of `user_sessions_path.rs`'s 210**
  plus `tddy_user_config`'s one reader. `pty_registry.rs` — six lines re-exporting `tddy_pty` — was
  moved and then **retracted untouched**, because nothing in families E, F, G or H reaches it; it
  was only ever needed by `pty_runtime`'s `PtyRuntime::spawn` half, which stays. `peer_forwarding`
  and `daemon_identity` are lifts of the same kind out of `livekit_peer_discovery` (2,369 lines),
  forced by five of the eight `host.HostService` methods routing to a peer *before* authenticating.
  Every origin module re-exports every lifted name, so no caller in `tddy-daemon` changed and there
  stays exactly one definition of each.
- **`branch_owner` takes a port rather than carrying `session_reader` with it.** A branch belongs
  to a worktree, which is family H; what *claims* one is a session, which is family C and stays in
  the daemon deliberately. `tddy_worktree_service::branch_owner::SessionListing` is the seam, and
  what crosses it is `SessionClaim` — the three fields the ownership rule judges on — rather than
  everything a session is. `tddy-daemon`'s `session_reader::DaemonSessionListing` implements it.
- **The file budget is best-effort, by agreement.** Seams are cut where they are cohesive. Whatever
  stays over 500 lines is listed in `## Scope`'s file-budget item with a reason, rather than split to
  hit a number at the cost of cohesion.

## Technical Debt & Production Readiness

- [ ] `tddy-desktop` is outside the CI gate; its build is verified locally and stated as such
- [x] ✅ Two hand-written tonic adapters are added for the UDS path because `tddy-codegen`'s
      `generate_tonic_adapter` is a stub — `host_tonic_adapter.rs` and `worktree_tonic_adapter.rs`,
      each a literal `async fn` per method (a macro cannot be used: `#[tonic::async_trait]` rewrites
      signatures and cannot see through one). All three services are mounted on the **same** socket
      by one `Server::builder()`, so a caller that reached `GetHostTooling` over the local socket
      before the split still does. **The debt stands**: generating them is out of scope and belongs
      in `docs/dev/todo/`
- [x] ✅ Relocated members come out `pub(crate)` and stay there by design. **Nine widenings had to
      stand**, each forced by a caller that stayed behind:

      | Symbol | Was | Now | Forced by |
      |---|---|---|---|
      | `tddy_daemon_kernel::spawn_as_user::resolve_livekit_room_name` | `pub(crate)` | `pub` | `connection_service.rs`, `connection_service/terminal_bridge_impl.rs`, `telegram_session_control.rs` |
      | `tddy_daemon_kernel::spawn_as_user::resolve_tool_path` | private | `pub` | `tddy-daemon`'s own `spawner.rs`, after the lift |
      | `tddy_daemon_kernel::spawn_as_user::resolve_relative_to_daemon_toolchain_root` | private | `pub` | the same |
      | `tddy_daemon_kernel::config::default_spawn_startup_grace_period_ms` | `pub(crate)` | `pub` | `spawner.rs`'s startup watch |
      | `tddy_daemon_kernel::config::default_spawn_startup_poll_interval_ms` | `pub(crate)` | `pub` | the same |
      | `tddy_worktree_service::worktree_files::canonicalize_root` | `pub(crate)` | `pub` | `context_files.rs` (family K, stays) |
      | `tddy_worktree_service::worktree_files::validate_rel_path_shape` | `pub(crate)` | `pub` | `context_files.rs`, `context_sync.rs` |
      | `tddy_worktree_service::worktree_files::git_listed_files` | `pub(crate)` | `pub` | `host_documents.rs` |
      | `tddy_host_service::host_prompts::answer_before_expiry` | `pub(crate)` | `pub` | `screen_sharing_service.rs` (node 2's, stays) |

      Two further symbols became `pub` as **new API rather than a widening**:
      `ConnectionServiceImpl::routing_view` (the config, roster and token resolver every routing
      decision is made from — `runtime.rs` hands the same three to `tddy-host-service`) and
      `connection_tonic_adapter::to_tonic_status` (three adapters over one socket must map a refusal
      to the same tonic code, and three copies of that match is three chances to drift)
- [x] ✅ **`impl ModuleReferences for RustBackend` is now covered** by
      `tests/move_module_to_crate_acceptance.rs` and `tests/rename_cross_file_acceptance.rs`, which
      drive a live rust-analyzer over a real three-crate fixture and end in `cargo check`. They run
      in the CI gate (~13.5s of a ~150-minute job), are **not** `#[ignore]`d, and fail rather than
      skip when rust-analyzer is absent. Serialised two ways: a `rust-analyzer` test group in
      `.config/nextest.toml` across processes, and a lock in the harness within one binary, since
      plain `cargo test` never reads that file
- [ ] The operation's **refusals** (cycle, undeclared module, nested module) are unit-tested only —
      proving a refusal against a live server costs a spawn for a decision made before the server is
      consulted
- [ ] A caller reaching the module as `use crate::host_registry;` then `host_registry::X` is **not**
      re-pointed: the survey asks references per item, so the module-level import is outside the
      reference set. Covering it needs a second engine call on the `mod` declaration, and a test that
      can only be written against a live server
- [ ] A `crate::` path in the moved file's **function bodies** is left alone; only `use` declarations
      are re-pointed. A build after the move surfaces it. Belongs in
      `docs/ft/coder/rust-code-restructuring.md` § Known limitations
- [ ] **`move_module_to_crate` evaluates each op against the *pre-move* tree, so a multi-op plan is
      not seen as a whole.** A 21-op plan refused on op 0 because `host_registry.rs` named
      `tddy_daemon::multi_host::…` — where `multi_host` was op 3 of the same plan, moving to the
      same destination crate. The workaround was to hand-layer the plan by dependency depth and run
      it once per layer, at ~15 minutes of rust-analyzer indexing each. Found on this operation's
      first real use
- [ ] **A cyclic module group cannot be moved by the operation at any layering.** It moves one
      module per op, so `host_tooling ⇄ ssh_agent` and `host_tooling ⇄ remote_desktop_probe` — the
      "classification cuts" `## Decisions & Trade-offs` names — are unreachable: each op sees the
      other module still in the origin crate. Six modules were hand-moved for this reason
- [ ] **A `pub use` facade in the origin makes the dependency-cycle refusal fire spuriously.** After
      `config` moved to the kernel, `tddy-daemon` kept `pub use tddy_daemon_kernel::config;`, so
      rust-analyzer canonicalises `crate::config::DaemonConfig` as `tddy_daemon::config::DaemonConfig`
      and the refusal reads it as an origin dependency the destination would depend back on. Eight
      moving modules had to be re-pointed at `tddy_daemon_kernel::config::` by hand first
- [ ] **The operation adds the destination as a dependency *of itself*** when the moved module names
      a sibling that has already moved to the same crate: `tddy-host-service` and
      `tddy-worktree-service` each came out of layer 2 with `tddy-<self> = { path = "" }` in their
      own manifest, which cargo rejects as a cyclic package dependency
- [ ] **It carries path dependencies but not registry ones.** The destination manifests were left
      short of `chrono`, `serde_yaml` and `futures-util`, all of which the moved files use
- [ ] **The journal is repo-scoped, not plan-scoped** (`.restructure/journal.jsonl`), so a
      *completed* plan blocks the next one with "a journal already exists for this plan — pass
      --resume", and `--resume` would resume the wrong plan. Each layer needed the journal archived
      by hand first
- [ ] Cosmetic: the operation appends one `pub use <crate>::*;` to the origin's `lib.rs` **per
      operation** rather than one per destination (ten identical lines after a ten-op plan), and
      appends `pub mod` lines after whatever the destination's `lib.rs` already said
- [ ] Nested modules and crate roots are **refused, not guessed** — only `<crate>/src/<module>.rs`
      moves, because a nested module's `mod` line lives in a file this operation would have to guess at

## Baseline

Recorded before any change; the acceptance criterion for M9.

| Gate | Before | After |
|---|---|---|
| `cargo build --workspace` | ✅ clean (5m10s) | ✅ clean (1m27s warm) |
| `cargo fmt --all --check` | ✅ clean | ✅ clean |
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites | **583 passed / 0 failed** (lib, `--test-threads=1`) — 226 unit tests travelled into the new crates |
| `cargo test -p tddy-daemon --no-fail-fast` | **2124 passed / 39 failed**, 169 suites | **1866 passed / 21 failed** — see the failure ledger below |
| `./test -p tddy-code-restructuring` | ✅ 251 passed / 0 failed | ✅ **287 passed / 0 failed** |
| `cargo test -p tddy-daemon-kernel` | n/a — crate did not exist | ✅ 85 passed / 0 failed |
| `cargo test -p tddy-host-service` | n/a — crate did not exist | ✅ 129 passed / 0 failed |
| `cargo test -p tddy-worktree-service` | n/a — crate did not exist | ✅ 81 passed / 0 failed |
| `cargo test -p tddy-service` | ✅ 104 passed, **2 failed** (the completion criterion) | ✅ **109 passed / 0 failed** |
| `cargo clippy -p tddy-daemon --all-targets -- -D warnings` | ✅ exit 0 | ✅ exit 0 |
| `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` | ✅ exit 0 | ✅ exit 0 |
| `cargo clippy` — the two new crates + kernel + service | n/a | ✅ exit 0 |
| `scripts/generated-code.sh check` | ⛔ did not exist; drift undetected | ✅ exit 0, all four directories |
| `./dev bun run --filter tddy-web test:unit` | not run | ✅ 1178 passed / 0 failed |
| `./dev bun run --filter tddy-web cypress:component` | not yet run — no web change in commit 2 | ✅ **231 specs, 1419/1419**, 5m34s |

**No test was lost to the split.** 583 (daemon lib) + 85 (kernel) + 129 (host) + 81 (worktree) = **878**,
against 828 before (809 + 19). The daemon's whole-package figure falls for the same reason and by the
same arithmetic.

### The move's own proof — `restructure verify --against bb0695b0`

`## Testing Plan` makes this, plus the moved-line diff, the proof of the mechanical move rather than
a test. Recording the result, including that it is **not zero**:

```
341,794 statements before, 341,882 after
plan is malformed: 50 statement(s) the tree lost and 138 it gained
```

Exit 1, and **no logic statement is among them**. The differences, all of them:

| n | What | Why |
|---|---|---|
| 31 | `crate::X` → `tddy_daemon_kernel::X` qualifier re-points | a module that changed crates cannot keep a `crate::` qualifier |
| 9 | doc-link rewrites | `[\`crate::foo\`]` follows the same move |
| 5 | `pub(crate)` → `pub` | the recorded widenings above |
| 2 | `local_hostname_or_local` + `process_startup_unix_ms_suffix` consolidated into `daemon_identity` | two derivations of "what is this machine called" became one |
| 2 | `include_str!` path re-points | two tests read a moved file |
| 4 | `#[must_use]` on the new `daemon_identity` functions | new attributes on newly-extracted code |

Corroborated by the diff shape: **37 files, +617/−390, nine of them moving with a zero-line diff**,
and every moved file 85–100% similar by git's own rename detection. A cross-crate move cannot change
zero tokens — the qualifier at the head of every moved `use` changed meaning by definition — so the
PRD's "no lost or gained statements" was the wrong criterion, and is corrected there.

### Failure ledger — all 21, none introduced by this node

| n | Failure | Status |
|---|---|---|
| 17 | panic at `connection_service/svc_resolve_tddy_tools_path.rs:163` — `self_arc called before set_self_handle` | **Pre-existing**, the known failure recorded below. It reaches more suites than the 1-of-25 the truncated baseline showed only because `--no-fail-fast` gets past binary 25 |
| 2 | `sandbox_session_stdio_acceptance` | **Pre-existing and stale**: the test `include_str!`s `src/connection_service.rs` and greps it for `"--stdio"`, a string at zero occurrences there before this PR as well — the spawn argv moved to `connection_service/svc_start_sandboxed_*.rs` in an earlier commit and the anchor was never updated |
| 1 | `sandbox_stdio_seatbelt_acceptance` — tool dispatch timeout in a real Seatbelt jail | **Environmental** |
| 1 | `session_room_acceptance` | **Environmental** — LiveKit container/port contention |

The 13 `tddy-remote-git-repo not built` failures in the *before* column are gone for an environmental
reason, not a code one: `cargo test` does not build that binary, and a `cargo build --workspace` in
this session did. They are a property of the runner, not of the tree.

The `-p tddy-daemon` figure matches the base branch's own recorded baseline (1027/1) exactly, which is
the evidence that rebasing this node onto #468's rewritten tip cost nothing.

**Known pre-existing failure, recorded before any change so it cannot later read as a regression:**

```
cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available
  panicked: ConnectionServiceImpl::self_arc called before set_self_handle
```

Same root cause as the five `sandbox_behavior_acceptance` failures known to fail on `master` — a
missing `set_self_handle` in the test's own construction of `ConnectionServiceImpl`, not a sandbox
regression. Expected to survive this node unchanged and stay at exactly one.

Per `tddy-coder-scoped-verification`, verification is scoped to the packages this node touches; the
full workspace carries pre-existing noise unrelated to this change.

## Final Checklist

Executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md` — the release-note file
- [ ] Move `host-registry.md`, `host-tooling-probe.md`, `host-add-key.md`, `worktrees.md` and
      `remote-git-service.md` from `packages/tddy-daemon/docs/` to the new packages' docs
- [ ] `packages/tddy-daemon/docs/connection-service.md` — remove the 17 moved endpoint entries and
      point at the two new services
- [ ] `docs/ft/coder/rust-code-restructuring.md` — the `Operations` table gains `move_module_to_crate`;
      `Known limitations` loses "no cross-crate move"
- [ ] Close `docs/dev/todo/2026-08-29-connection-service-rs-is-19-600-lines.md`,
      `2026-09-06-connection-service-rs-is-22800-lines.md` and
      `2026-09-06-server-rs-run-server-takes-12-positional-arguments.md`
- [ ] New `docs/dev/todo/` entries: versioned proto package names; generating the tonic adapters
- [ ] Doc triage over every moved path and symbol:
      `grep -rn -e 'connection_service' -e 'host_registry' -e 'worktrees' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon`
