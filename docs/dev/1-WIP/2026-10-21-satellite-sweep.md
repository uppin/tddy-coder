# Changeset: Split the files the `#satellite` stack grew over the 500-line budget, and close the records it added

**Date**: 2026-10-21
**Status**: 🚧 In Progress — planned; 21 plans proven with `restructure check --deep` on `bcdbb08af`; awaiting the developer's review (`6e`)
**Type**: Refactor
**Complexity**: 5 — eleven crates (`tddy-daemon`, `tddy-service`, `tddy-session-sync`, `tddy-session-tool-client`, `tddy-terminal-rpc`, `tddy-telegram-control`, `tddy-tools`, `tddy-livekit`, `tddy-host-service`, `tddy-session-activity`, `tddy-daemon-auth`), fourteen backlog items in 21 engine plans, the daemon's composition root (`runtime::build`, 1,080 lines, one function) to decompose without adding a parameter-count finding — an unsettled design (D1) — and three items the engine refuses (`build.rs`, the `cfg` pairs in `cli.rs`, an early `return` and a parameter count in `build_auth_entries_admitting`)
**Stack**: `#satellite` 16/16 — the sweep node. Branch `feature/satellite/sweep`, base `master` `bcdbb08af` (every node 1–15 has merged; see Dependencies)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-10-21-satellite-sweep-initial-discovery.md](./2026-10-21-satellite-sweep-initial-discovery.md).

State A below is distilled from that file: the collect method, every before → after measurement, the
per-file seam reading and the engine limits are there.

## Scope decision (the developer's, 2026-10-21)

**"What the stack added or grew"**: the files this stack grew past or further beyond the
500-production-line budget, the code-issue records and TODOs the stack itself added, and the
stack-shared files `[W]` declined to split. Pre-existing records on files the stack **shrank or left
unchanged** keep their records and are not swept here (§ Excluded). Not to be widened.

## Prerequisites

Every backlog item, by record path, with the verdict this node takes. **✅ RESOLVED HERE** marks what
this node's commits delete once the re-measure confirms it; nothing is deleted at planning time.

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` — 1,781 → 1,867 (stack +80) | **✅ RESOLVED HERE** — needs the `build` design decision (D1) | seams A–C by `move_item` (plans 19–21, proven: 1,867 → ~1,355), then `build` cut (D1; probe-b proves a cut inside `build` is accepted) |
| `packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md` — 1,010 → 1,080 | **✅ RESOLVED HERE** — same decision (D1) | `build` ≤ 60 lines, every extracted helper ≤ 60 lines and ≤ 5 parameters |
| `packages/tddy-service/docs/code-issues/oversized-file-build.md` — 723 → 743 (stack +14) | ⚠ **needs developer consent** (D2) | engine refused: `build.rs is not under packages/tddy-service/src, so it is no module of the package` — no plan can be written |
| `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` — 520 → 555 (+35) | **✅ RESOLVED HERE** | plan 01 |
| `packages/tddy-session-sync/docs/code-issues/oversized-file-sync.md` — 788 (record added by #629) | **✅ RESOLVED HERE** | plans 02, 03 |
| `packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md` — 1,111 → 1,114 (+3) | **✅ RESOLVED HERE** | plans 04, 05 |
| `packages/tddy-terminal-rpc/docs/code-issues/oversized-file-pty-relay.md` — 775 → 784 (+9; added by #623) | **✅ RESOLVED HERE** | plans 06–08 |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-mod.md` — 539 → 557 (+18) | **✅ RESOLVED HERE** | plan 09 |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-session-start.md` — 713 → 719 (+6) | **✅ RESOLVED HERE** | plan 10 (`move_impl_members` of the four contiguous members from `handle_telegram_claude_model_callback` on; the two spawns move whole, not cut) |
| `packages/tddy-tools/docs/code-issues/oversized-file-cli.md` — 895 (added by #690) | ⚠ **needs developer consent** (D6) | plans 11, 12 take it to ~663; the rest are `cfg(unix)` / `cfg(not(unix))` pairs the engine refuses (probe-a) |
| `packages/tddy-livekit/docs/code-issues/oversized-file-participant.md` — 956 → 907 (added by #630) | **✅ RESOLVED HERE** | plans 13, 14 |
| `packages/tddy-host-service/docs/code-issues/oversized-file-host-registry.md` — 596 (added by #627) | **✅ RESOLVED HERE** | plan 15 |
| `packages/tddy-session-activity/src/service.rs` — 819 → 835 (+16, #635) — **no record** | **✅ RESOLVED HERE** — ⚠ `service/rpc.rs` lands at ~495 (D3) | plans 16, 17 (the whole `#[async_trait]` impl moves; accepted by `check --deep`). No record is written: the finding is closed in the same node that found it, and the final measurement goes into the change-history entry |
| `packages/tddy-daemon-auth/docs/code-issues/complexity-auth-build-auth-entries-admitting.md` — 110 → 111 lines, 5 → 6 params | ⚠ **needs developer consent** (D4) | the engine reaches ~80 lines (plan 18, proven); the rest is an early `return` and a parameter object |
| [`2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md`](../todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md) | ⚠ **NARROWED HERE** | every row this node's scope covers closes with its record; the `session_room.rs`, `livekit_peer_discovery.rs` and `connection_service.rs` rows were net-shrunk by the stack and live on in their records; the entry keeps only its non-size paragraph (Telegram's `warn!` for an unwired exposure — an external-dependency decision) and goes to the TODO gate |
| [`2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md`](../todo/2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md) | **✅ RESOLVED HERE** | all three rows (`attach.rs`, `lib.rs`, `pty_relay.rs`) and the `sync.rs` note close with plans 01–08 |
| [`2026-10-19-restructure-extract-method-refused-under-async-trait.md`](../todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md) | ⚠ DURING — engine gap | plan 17 moves the impl whole instead of cutting into it (accepted) |
| [`2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root.md`](../todo/2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root.md) | ⚠ DURING — engine gap | the same resolution rule refuses `build.rs` (D2) |
| [`2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate.md`](../todo/2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate.md) | — not met | no plan moves a binary root or a `tests/*.rs` suite |
| [`2026-10-19-terminal-exposure-lock-spans-the-join.md`](../todo/2026-10-19-terminal-exposure-lock-spans-the-join.md), [`2026-10-21-session-delta-stores-adapter-lives-in-the-daemon.md`](../todo/2026-10-21-session-delta-stores-adapter-lives-in-the-daemon.md), [`2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin.md`](../todo/2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin.md) | Out of scope — TODO gate | behaviour, a new crate edge (in flight in #694) and a Cargo feature; no restructure closes them |

## Responsibility

The module layout of twelve files and the length of two functions, with no behaviour change:

- `tddy-daemon/src/runtime.rs` → `runtime.rs` + `runtime/{tasks,env,telegram_wiring}.rs` + the topic
  modules `build` is cut into (D1). `first_login_enrolment` stays: its sibling `this_process_os_user` is a
  `cfg` pair (D6's limit) and the 42 lines are not needed.
- `tddy-session-sync/src/attach.rs` → + `attach/daemon_http.rs`; `sync.rs` → + `sync/{git,delta}.rs`.
- `tddy-session-tool-client/src/lib.rs` → + `room_transport.rs`, `rpc_transports.rs`.
- `tddy-terminal-rpc/src/pty_relay.rs` → + `pty_relay/{room,daemon_http,grpc_terminal}.rs`.
- `tddy-telegram-control/src/telegram_session_control/` → + `types.rs`, `cli_spawns.rs`.
- `tddy-tools/src/cli.rs` → + `cli/{session_tools,changeset_commands}.rs`; the `cfg`-paired relay families
  only with D6's consent.
- `tddy-livekit/src/participant.rs` → + `participant/{publisher,serve}.rs`.
- `tddy-host-service/src/host_registry.rs` → + `host_registry/sightings.rs`.
- `tddy-session-activity/src/service.rs` → + `service/{ports,rpc}.rs`.
- `build_auth_entries_admitting` shortened by three `extract_method`s (and, with consent, D4).
- `tddy-service/build.rs` only with consent (D2).
- At wrap: every resolved record deleted with its final measurement first recorded in the change-history
  entry; the narrowed TODO edited; the `Location` of the moved-but-open complexity records updated.

## Boundaries

- **No behaviour change**, no new public symbol beyond module paths and extracted helpers, no test
  semantics changed, no new dependency, no new Cargo feature, no proto change.
- **Every move is the engine's** (`tddy-tools restructure`). No `git mv`, no hand-moved code. A seam the
  engine refuses is listed with its refusal and waits for the developer's consent (D1–D4, D6); any hand
  fix after an apply is limited to the build errors the move itself caused and carries a
  `TODO(restructure-engine)` marker.
- **Adds no code issue.** Every new module and every extracted helper is under 500 production lines,
  60 lines per function and 5 parameters. A seam that would breach one is not applied.
- Does not touch the files the stack net-shrank (§ Excluded), `tddy-host-service/src/service.rs`
  (#693's), or anything #694 edits.
- Does not take any TODO-gate item.

## Dependencies

What each node delivered that this one inherits. **All fifteen are MERGED into `master`**; this node
consumes nothing unmerged — only the code they left — and its base is `master` (`bcdbb08af`).

| Node | PR (squash on `master`) | What it left that this node restructures | This node does NOT |
|---|---|---|---|
| n1 extension ports | #623 — `97d0ec7d4` | integration board + `SessionReach` wiring in `runtime::build`; `open_session_room` in `attach.rs`; `build.rs` passes; `pty_relay.rs` +2 | change a port or the board |
| n2 peer forwarding | #624 — `8b2c945bf` | `LiveKitPeerForwarder` in `build` | change forwarding |
| n3 session rooms | #625 — `33115ad38` | room port wiring in `build`, Telegram `mod.rs` | change the rooms port |
| n4 room tokens | #626 — `dc84ebe8c` | `room_token_minter` in `build`; `minter` parameter of `build_auth_entries_admitting` | change token minting |
| n5 core modules | #627 — `0ec8c3c33` | `host_registry.rs` in `tddy-host-service` | move it again |
| n6 terminal exposure | #628 — `9f4bc96c5` | `terminal_exposure` in `build`/`build_telegram`, Telegram `mod.rs` + `session_start.rs` | change the exposure or its lock (TODO gate) |
| n7 client transports | #629 — `fcf1a063c` | `join_session_room` (`attach.rs`), `sync.rs` port, `room_session_via` (`lib.rs`), `join_terminal_room` (`pty_relay.rs`) | change the client room port |
| n8 coder participant | #630 — `e541c5cab` | `participant.rs` | change the participant |
| n9 web settings | #631 — `ffe71a895` | nothing here | touch `tddy-web` |
| n10 forwarding consumers | #632 — `4ddd89c16` | OAuth tunnel in `RuntimeTasks` | change the tunnel |
| n11 agent clones | #633 — `479393523` | nothing here | touch `tddy-session-agents` |
| n12 split transport | #634 — `569c65456` | `split_room_hosting` + `.with_split_rooms` | change the split port |
| n13 lifecycle launch | #635 — `0bee1bf79` | `with_livekit_room_ports` call in `build`; `NoSessionDeltaStores` in `session-activity` | move the delta adapter (#694's) |
| n14 client bins wiring | #689 — `35fcc585f` | the `tddy-session-sync` library/`-cli` split | touch `tddy-session-sync-cli` |
| n15 tools wiring | #690 — `bcdbb08af` | `cli.rs` re-parented into the `tddy-tools` library | touch `tddy-tools-cli` or `server.rs` |

## Draft PR contract

A restructure has no surface to publish ahead of its moves — the move is the implementation. The draft
publishes **the plan and the green baseline**: the plan files under
[`2026-10-21-satellite-sweep-plans/`](./2026-10-21-satellite-sweep-plans/), the module layout below, and
the baseline the applied tree is held to by failing-test **name**.

## Green wave

**Wave:** the tail, alone
**Greenable independently:** yes — every node below is merged; the oracle is the existing suites of the
eleven packages it edits.
**Concurrent with:** none in this stack. Outside it: #694 (`#satellite` follow-up, draft) and #693
(`#host-desktops` 10/10, draft) — see Decisions D5.
**Blocks:** nothing.

## Affected Packages

- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — `runtime` becomes a module directory; `tddy_daemon::runtime::{build, apply_env_overrides, split_room_hosting, spawn_common_room_discovery_task, DaemonRuntime, RuntimeOptions, RuntimeHost, …}` keep resolving
- **tddy-session-sync**, **tddy-session-tool-client**, **tddy-terminal-rpc**, **tddy-telegram-control**, **tddy-tools**, **tddy-livekit**, **tddy-host-service**, **tddy-session-activity**: module directories; every path another package names keeps resolving through `reexport: "outside"` / `"glob"` facades
- **tddy-daemon-auth**: helpers extracted from `build_auth_entries_admitting`
- **tddy-service**: only with D2's consent

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

Twelve files the stack grew or recorded are cut into topic modules by `tddy-tools restructure`, and
`runtime::build` is cut into helpers. Every path a caller outside its crate uses keeps resolving through
a facade, so no other package's source is edited.

## Background

Every `#satellite` node grew a file that was already over budget, or recorded one it touched, and
deferred the split "until the stack lands" because a sibling node edited the same file
(`docs/dev/todo/2026-10-17-satellite-1-…`, `2026-10-19-satellite-7-…`, and the `runtime.rs` rows of
#634/#635 that say "the stack's sweep node owns this file's decomposition"). The stack has landed, so
nothing downstream shares these files any more.

## Scope

- [x] **Collect**: backlog measured against `fb9879af8` (discovery § 1)
- [x] **Plan**: layout proposed, seam order fixed, 21 plans emitted from `restructure anchors` and proven with `check --deep`
- [x] **Baseline recorded** (1,524 passed, 0 failed, 3 ignored across eleven packages)
- [ ] **Consent** (`6e`): D1–D6
- [ ] **Apply**: per plan, `check --deep`, dry run, apply, `verify --against HEAD`, one commit per plan
- [ ] **Baseline after**: the same packages back to the recorded failing-test names
- [ ] **Code Quality**: `cargo fmt --check`; `cargo clippy -p <touched> --all-targets -- -D warnings`
- [ ] **Re-measure** (`[W]`): § How `[W]` re-measures
- [ ] **Documentation**: Final Checklist executed at wrap

## Technical Changes

Production lines counted to the first `#[cfg(...test...)]`. Projected figures are whole-item arithmetic
plus facade lines.

### State A

| File | Production lines | What is over |
|---|---:|---|
| `tddy-daemon/src/runtime.rs` | 1,867 | `build` alone 1,080 |
| `tddy-session-tool-client/src/lib.rs` | 1,114 | |
| `tddy-livekit/src/participant.rs` | 907 | one 545-line `impl` |
| `tddy-tools/src/cli.rs` | 895 | |
| `tddy-session-activity/src/service.rs` | 835 | one 481-line `#[async_trait]` impl |
| `tddy-session-sync/src/sync.rs` | 788 | |
| `tddy-terminal-rpc/src/pty_relay.rs` | 784 | |
| `tddy-service/build.rs` | 743 | `main` alone 682 |
| `tddy-telegram-control/…/session_start.rs` | 719 | one 714-line `impl` |
| `tddy-host-service/src/host_registry.rs` | 596 | |
| `tddy-telegram-control/…/mod.rs` | 557 | |
| `tddy-session-sync/src/attach.rs` | 555 | |
| `tddy-daemon-auth` `build_auth_entries_admitting` | 111 lines, 6 params | |

### State B — module layout

Projected from whole-item spans; the apply's own numbers replace them.

| File | Moves out | Projected |
|---|---|---:|
| `runtime.rs` | A `tasks` (~296), B `env` (~81), C `telegram_wiring` (~135) — then `build`'s groups (D1) | ~1,355 before D1; < 300 after |
| `attach.rs` | `daemon_http` (~185) | ~370 |
| `sync.rs` | `git` (~130), `delta` (~205) | ~455 |
| `session-tool-client/lib.rs` | `room_transport` (~296), `rpc_transports` (~380, includes `livekit_session`: a move into `room_transport` would be a second op into a module the first op creates, which `check --deep` cannot see) | ~440 |
| `pty_relay.rs` | `room` (~177), `daemon_http` (~150), `grpc_terminal` (~190) | ~270 |
| `telegram_session_control/mod.rs` | `types` (~145, `reexport: "glob"`) | ~410 |
| `telegram_session_control/session_start.rs` | `cli_spawns` (~420, `move_impl_members`) | ~300 |
| `cli.rs` | `session_tools` (~166), `changeset_commands` (~66); the four `cfg`-paired relay families only with D6 | ~663 (D6 for the rest) |
| `participant.rs` | `publisher` (~155), `serve` (~330, `move_impl_members`) | ~425 |
| `host_registry.rs` | `sightings` (~137) | ~460 |
| `session-activity/service.rs` | `ports` (~135), `rpc` (the whole 481-line `#[async_trait]` impl) | ~220; `rpc.rs` ~495 (D3) |
| `build_auth_entries_admitting` | three `extract_method`s | ~80 (D4 for the rest) |

### Delta — plans in apply order

[`2026-10-21-satellite-sweep-plans/`](./2026-10-21-satellite-sweep-plans/) — every file emitted from
`restructure anchors` (no hand-written line numbers, except probe-a's range, which is the probe) and
proven with `restructure check --deep` against the warm index on `bcdbb08af` (all `no findings`
except probe-a). One plan per commit at apply time. A plan's anchors are item anchors, so an earlier
plan's apply in another file does not stale it; plans on the **same** file are applied in the order
listed and re-checked (`check --deep`) before each, since `check` sees today's tree only.

| # | Plan | Op(s) | Destination |
|---|---|---|---|
| 01 | `01-session-sync-attach-daemon-http` | `move_item` ×1 (6 items) | `attach::daemon_http` |
| 02 | `02-session-sync-sync-git` | `move_item` (8) | `sync::git` |
| 03 | `03-session-sync-sync-delta` | `move_item` (11) | `sync::delta` |
| 04 | `04-session-tool-client-room-transport` | `move_item` (20) | `room_transport` |
| 05 | `05-session-tool-client-rpc-transports` | `move_item` (13) | `rpc_transports` |
| 06 | `06-terminal-rpc-pty-relay-room` | `move_item` (2) | `pty_relay::room` |
| 07 | `07-terminal-rpc-pty-relay-daemon-http` | `move_item` (4) | `pty_relay::daemon_http` |
| 08 | `08-terminal-rpc-pty-relay-grpc-terminal` | `move_item` (2) | `pty_relay::grpc_terminal` |
| 09 | `09-telegram-mod-types` | `move_item` (17), `glob` | `telegram_session_control::types` |
| 10 | `10-telegram-session-start-cli-spawns` | `move_impl_members` (4) | `telegram_session_control::cli_spawns` |
| 11 | `11-tools-cli-session-tools` | `move_item` (6) | `cli::session_tools` |
| 12 | `12-tools-cli-changeset-commands` | `move_item` (3) | `cli::changeset_commands` |
| 13 | `13-livekit-participant-publisher` | `move_item` (4) | `participant::publisher` |
| 14 | `14-livekit-participant-serve` | `move_impl_members` (4) | `participant::serve` |
| 15 | `15-host-service-host-registry-sightings` | `move_item` (7) | `host_registry::sightings` |
| 16 | `16-session-activity-service-ports` | `move_item` (9) | `service::ports` |
| 17 | `17-session-activity-service-rpc` | `move_item` (the trait impl) | `service::rpc` |
| 18 | `18-daemon-auth-build-auth-entries-extracts` | `extract_method` ×3, bottom-up | free fns in `auth` |
| 19 | `19-daemon-runtime-tasks` | `move_item` (12) | `runtime::tasks` |
| 20 | `20-daemon-runtime-env` | `move_item` (4) | `runtime::env` |
| 21 | `21-daemon-runtime-telegram-wiring` | `move_item` (3) | `runtime::telegram_wiring` |
| probe-a | `probe-a-tools-cli-cfg-pairs` | `extract_module` (range) | **refused** — evidence for D6, not applied |
| probe-b | `probe-b-daemon-runtime-build-vm-group` | `extract_method` | accepted — evidence for D1, applied only as part of D1's plan |

Totals: 21 plans, 23 operations — `move_item` 18, `move_impl_members` 2, `extract_method` 3; plus two
probes.

## Callers

| | |
|---|---|
| External importers | by `grep -rl '<crate>::<module>'` over `packages/` (a grouped `use <crate>::{…}` is not counted — `check --deep` is the authority): `tddy_daemon::runtime` — 17 files in `tddy-daemon` (`main.rs`, `tests/`), `tddy-desktop`, `tddy-e2e`, `tddy-daemon-kernel`, `tddy-session-tool-client`, `tddy-code-restructuring` (1 each); `tddy_session_sync::{attach,sync}` — `tddy-session-lifecycle` (tests); `tddy_session_tool_client::` — 9 crates; `tddy_terminal_rpc::pty_relay` — `tddy-terminal-rpc` (3), `tddy-tools`, `tddy-sandbox-app`; `host_registry::` — 7 crates; `tddy_session_activity::service` — `tddy-session-lifecycle` (3); `build_auth_entries_admitting` — `tddy-daemon` (1) |
| Facade | `reexport: "outside"` on every `move_item` (a facade only for what another package or a `tests/` file reaches); `"glob"` for the Telegram DTOs (`use super::*` children); `move_impl_members` and `extract_method` need none |
| Consequence | no caller outside the moved file's crate is edited |

## Visibility

What `check --deep` reports each apply will widen (the run re-reports; the table is updated from it):

| Plan | Widened | To | Why |
|---|---|---|---|
| 01 | `DaemonHttp` and `new`, `refresh_session`, `list_sessions`, `open_session_room` | `pub(super)` | `attach` still calls them |
| 04 | `installed_room_connector`, `livekit_room_cache`, `worktree_activity_line` | **`pub(crate)`** | the first two are called by `livekit_session` (L817–824), which plan 05 puts in the sibling `rpc_transports`; `worktree_activity_line` by the crate root's inline tests (L1137, L1154) |
| 05 | `execute_tool_request` | **`pub(crate)`** | `conversation.rs` (L20, L426) and `room_transport::dispatch_via_livekit` call it |
| 06–08 | `run_livekit_session`, `exchange_stub_session_token`, `connectrpc_post`, `run_grpc_terminal` | `pub(super)` | `run_pty_relay` and the siblings call them |
| 11 | `all_session_tools`, `ListToolsEntry::name` | `pub(super)` | |
| 13 | `spawn_response_drain`, `spawn_response_drain_reconnectable`, `SharedPublisher::{new,update}` | `pub(super)` | |
| 15 | `apply_sighting`, `apply_departure` | `pub(super)` | `FileHostRegistry` calls them |
| 19 | the four private task structs and every field of `DaemonChildren`, `RuntimeTasks`, `CredentialSyncPublishWatch`, `OauthLoopbackTunnel`, `LocalSocketTransport`, `TelegramInbound` (22 items) | `pub(super)` | `build` constructs them by struct literal. Fields were private only by module, not to hold an invariant (plain wiring records) |
| 20 | `data_dir_from`, `tddy_data_dir_for` | `pub(super)` | |
| 21 | `build_telegram`, `TelegramWiring::{hooks,inbound}` | `pub(super)` | |

No widening past `pub(crate)`; three `pub(crate)` rows (04, 05).

## Baseline

Recorded on `bcdbb08af` before any change, one `cargo test -p <pkg> --no-fail-fast` per package, run
sequentially (`tddy-daemon` through `./test -p tddy-daemon`, which pre-builds the `tddy-tools` binary
its split-session suites spawn — `cargo test -p tddy-daemon` alone fails 18 of them with `tddy toolchain
binary "tddy-tools" not found in …/target/debug`, an environment fact since #690 moved the binary to
`tddy-tools-cli`, not a regression):

| Package | Passed | Failed | Ignored | Test binaries |
|---|---:|---:|---:|---:|
| `tddy-session-activity` | 49 | 0 | 0 | 4 |
| `tddy-daemon-auth` | 159 | 0 | 0 | 19 |
| `tddy-host-service` | 199 | 0 | 0 | 7 |
| `tddy-livekit` | 97 | 0 | 2 | 17 |
| `tddy-terminal-rpc` | 69 | 0 | 0 | 9 |
| `tddy-session-tool-client` | 16 | 0 | 0 | 3 |
| `tddy-session-sync` | 70 | 0 | 0 | 7 |
| `tddy-telegram-control` | 158 | 0 | 0 | 17 |
| `tddy-service` | 157 | 0 | 0 | 8 |
| `tddy-tools` | 266 | 0 | 0 | 22 |
| `tddy-daemon` (`./test -p`) | 284 | 0 | 1 | 34 |
| **Total** | **1,524** | **0** | **3** | |

**Known noise:** `tddy-daemon` `session_agent_remote_acceptance::restores_a_clone_that_diverged_and_says_so`
failed once in the full run (`a reconcile must be reported, never silent`, the binary took 414 s at
load average > 100) and passed alone in 14 s; it is counted as passed above. Not run here because the
sweep does not touch it: `tddy-session-lifecycle` (its 16 known sandbox failures are not in this
baseline).

## Decisions & Trade-offs

- **D1 — how `build` is cut (unsettled; the reason the score is 5).** `build` reads six to twenty of its
  own locals per group, so plain `extract_method` cuts yield helpers with more than five parameters —
  a new `complexity` finding each, which this node may not add. probe-b shows the engine accepts a cut
  (the VM group, 27 lines, four inputs) — so the limit is the design, not the tool. Options: (a) a
  hand-written `BuildContext` struct of the shared locals (wiring, written by hand first, like
  `read_fields_through`'s state type), then `extract_method` per group taking `&mut BuildContext`; (b) cut
  only where a group reads ≤ 5 locals and leave `build` above 60 (does not close
  `complexity-runtime-build`); (c) move `build` into its own node of this stack (`#satellite` 17/17), so
  this node carries the mechanical files and plans 19–21. **Recommendation: (c)**, with (a) as that node's
  design.
- **D2 — `tddy-service/build.rs`.** Engine refused (`build.rs is not under packages/tddy-service/src, so
  it is no module of the package`). Functions alone do not close the record — `build.rs` would still be
  ~750 lines — so a consent has to cover both halves: `main` cut into one `compile_<family>()` per proto
  group (`main` ~60), **and** those functions moved into `build/<family>.rs` files `#[path]`-included from
  `build.rs` (the record's own cure), proved with `./dev scripts/generated-code.sh check` and
  `cargo build -p tddy-service`. **Recommendation:** consent to that hand split, byte-identical bodies,
  one commit, `TODO(restructure-engine)` marker; the alternative is leaving the record open, which the
  sweep rule forbids (a 🔴 stop).
- **D3 — `service/rpc.rs` at ~495.** The whole `#[async_trait]` impl moves (plan 17, accepted), so no
  hand move is needed. The new file is the impl (481 lines) plus its imports: within budget by a margin
  of a few lines. If the apply lands it over 500, no further engine cut exists (a trait impl cannot be
  split, `E0119`); the fallback is the `#653` precedent (bodies into inherent `*_impl` methods by hand),
  which needs consent.
- **D4 — `build_auth_entries_admitting`.** The engine reaches ~80 lines (plan 18). Under 60 needs the
  `None => return Ok(…)` arm hoisted before the `match` (a `let … else`) by hand, then the match
  extracted; 6 → 5 parameters needs `web_host`/`web_port` folded into one value (e.g. the redirect
  fallback computed by the caller) — a signature change at every caller. Both are small hand edits; both
  need consent.
- **D5 — open PRs.** #694 (draft) edits `tddy-daemon/src/{lib.rs,livekit_host_ports.rs}`,
  `tddy-daemon-livekit` and `tddy-session-lifecycle/tests/` — no file this node moves; whichever lands
  second rebases over `tddy-daemon`'s change-history directory only. #693 (draft) edits
  `tddy-host-service/src/{service,host_tooling}.rs` and `lib.rs`; this node edits `host_registry.rs`
  only, and its new child is declared inside `host_registry.rs`. No other open PR touches a file in
  State A (`gh pr diff --name-only` over every open PR, 2026-10-21).
- **D6 — `cfg`-paired items in `cli.rs`.** Each `relay_*` (and `runtime.rs`'s `this_process_os_user`) is a
  `#[cfg(unix)]` / `#[cfg(not(unix))]` pair of same-named functions. An `items` anchor is refused (`names
  2 items … qualify it with the trait`; `#N` is accepted only for inherent impls), `--at` on the run is
  refused (`inside no item`), and a range `extract_module` over 738–866 is refused (`no import
  rust-analyzer offered for `args``, the inactive `cfg` arm is not analysed) — probe-a. The four families
  that hold a pair (relay submit/ask/spawn-conversation, transition, list/invoke actions, the
  not-unix `relay_ask`) are ~430 lines; plans 11–12 alone leave `cli.rs` at ~663. Options: a consented
  hand move of each pair with its callers (byte-identical), or an engine fix (an item-anchor spelling for
  `cfg` twins — a new engine TODO, which this node may not add). **Recommendation:** consented hand move,
  one commit, `TODO(restructure-engine)` marker.
- **Moved, still-open complexity records** keep their findings and change `Location`:
  `complexity-cli-run-call-tool` (→ `cli/session_tools.rs`), `complexity-participant-run`
  (→ `participant/serve.rs`), `complexity-service-stream-acp-replay` (→ `service/rpc.rs`).

## Excluded — measured, not swept

| File | Before → after | Why out |
|---|---|---|
| `tddy-daemon-livekit/src/session_room.rs` | 2,874 → 2,652 | stack net −222; record stands |
| `tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,658 → 1,390 | stack net −268; record stands |
| `tddy-session-lifecycle/src/connection_service.rs` | 527 → 516 | stack net −15 (+5 n12, −29 n13); record stands |
| `tddy-coder/src/run.rs` | 2,678 → 2,655 | shrank |
| `tddy-host-service/src/service.rs` | 937 → 972 | stack −1 (the +36 is #656's); #693 splits it |
| `tddy-session-agents/src/session_agent_clone.rs` | 1,157 → 1,156 | shrank |
| `tddy-daemon-rpc/src/project/coordinate_handlers.rs` | 658 → 652 | shrank |
| `tddy-daemon-auth/src/auth.rs` (file) | 622 → 619 | shrank |
| `tddy-worktree-service/src/service.rs` | 777 → 772 | shrank |
| `tddy-telegram-control/…/chaining_and_listing.rs` | 518 → 518 | unchanged |
| `tddy-session-files/src/service.rs` | 553 → 553 | unchanged; **has no record** (a gate miss, listed for the developer) |
| `tddy-session-lifecycle/src/connection_service/{agent_activity_unit_tests,stack_child_spawn_tests}.rs` | 1,269, 600 | test modules, unchanged |
| `tddy-testing-commons/src/dependency_graph.rs`, `tddy-daemon-extensions/src/lib.rs` | 165, 50 | within budget |

## How `[W]` re-measures

1. The same awk as discovery § 1b over every file in State A and every new module: each **before → after**
   in the change-history entry, every new file < 500, every function ≤ 60 lines and ≤ 5 parameters
   (brace matching).
2. For each ✅ record: write the final measurement into the change-history entry, **then** `git rm` the
   record in the commit that resolved it (`deferred-work`). A record only narrowed stays, with the new
   numbers and a narrowed *What would close it*.
3. `/analyze-code-issues` line-count pass over the eleven packages, diffed against the records at
   `bcdbb08af`: **no** new finding. Anything left of this backlog is a 🔴 stop, not a TODO.
4. Resolved TODOs deleted by the commit that resolves them; the narrowed one edited.

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `packages/<pkg>/docs/changesets/2026-10-21-satellite-sweep.md` per affected package — release-note
      entry with before → after numbers and the records it closed
- [ ] `docs/dev/changesets/2026-10-21-satellite-sweep.md` — cross-package entry
- [ ] `packages/tddy-tools/docs/code-issues/complexity-cli-run-call-tool.md`,
      `packages/tddy-livekit/docs/code-issues/complexity-participant-run.md`,
      `packages/tddy-session-activity/docs/code-issues/complexity-service-stream-acp-replay.md` — `Location`
- [ ] Docs naming a moved path: `grep -rn` the old module paths over each package's `README.md` and
      `docs/` (re-run against what landed)
