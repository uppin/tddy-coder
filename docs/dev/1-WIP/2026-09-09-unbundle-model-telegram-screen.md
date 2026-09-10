# Changeset: model registry, telegram and screen sharing in their own crates

**Date**: 2026-09-09
**Status**: ✅ Complete (wrapped 2026-09-10) — with two milestones explicitly partial, see below
**Type**: Refactor
**Stack**: `#unbundle` node **2 of 8**. PR [#471](https://github.com/uppin/tddy-coder/pull/471).
Base: `feature/unbundle/host-worktree-services` (node 1, PR #470)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-model-telegram-screen-initial-discovery.md](./2026-09-09-unbundle-model-telegram-screen-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

Three subsystems leave `tddy-daemon` for crates of their own. All three **already have their own
proto services**, so no RPC coordinate moves and no client migrates:

| New crate | Modules | prod LoC | Serves | Dedicated tests |
|---|---:|---:|---|---|
| `tddy-model-registry` | 13 (`model_registry/`) | 3,035 | `models.ModelRegistryService` (12), `acp.AcpService` (1) | 8 files / 4,618 LoC |
| `tddy-telegram` | 9 (planned) / **4 modules + 1 seam moved** | 7,237 | nothing — `PresenterObserver` / `PresenterIntent` are `tddy-service`'s | 12 files / 4,892 LoC (none moved) |
| `tddy-screen-sharing` | 2 | 2,299 | `screen_sharing.ScreenSharingService` (10), `screen_sharing_input` (1) | 2 files / 624 LoC |

It also **deletes 1,008 lines of unreachable code**: `vnc_service.rs` (204), `vnc_vault.rs` (359) and
their two acceptance suites (445). `runtime.rs` registers `screen_sharing.ScreenSharingService` but
never `vnc.VncService`; `grep -rn 'VncService'` across `packages/` finds only `lib.rs:111`, the two
source files and the two test files. The deletion is here rather than in its own node because this is
the node whose reviewer is already reading the screen-sharing subsystem, which is the only context in
which "the VNC service is dead, and here is the live one beside it" is checkable.

## Boundaries

This PR explicitly does **not**:

- Change any proto. All three subsystems keep the service coordinates they have. `vnc.proto` is left
  in place even though its service is deleted — retiring the proto is a separate decision, recorded
  in `docs/dev/todo/` at wrap.
- Touch `connection.ConnectionService`. Node 1 took families E–H; families B, I–N and P–T belong to
  nodes 4 and 6–8.
- Move `host_documents.rs`, despite the `host_` prefix — it is the context/documents subsystem and
  belongs to node 6.
- Move the elicitation modules' *callers*. `active_elicitation.rs` and `elicitation.rs` move with
  telegram because that is where their behaviour lives, but `session_list_enrichment.rs` and
  `session_notifications.rs` reach them and stay behind this node; they consume the new crate.
- Implement `move_module_to_crate`. That is node 1's, and this node is its second real user.
- Force every file under 500 lines. `telegram_session_control.rs` (4,472 measured, not 4,158),
  `telegram_notifier.rs` (1,948 measured, not 1,846), `screen_sharing_service.rs` (1,953),
  `model_registry/store.rs` (1,075), `telegram_bot.rs` (788 measured, not 737)
  and `model_registry/acp_service.rs` (617) are all over budget and are split where the seams
  are cohesive; whatever stays over is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate` in `packages/tddy-code-restructuring` — the operation, its plan schema, the manifest edits and the crate-level `pub use` facade | every module move in this PR is a plan the operation executes; the facade is what keeps the caller diff at zero | add, extend or fix the operation; a defect found here is reported upward and fixed on node 1's branch |
| `n1` host-worktree-services | `tddy-daemon-kernel` exporting `now_unix_ms()` | `telegram_session_subscriber.rs:122` calls it, and it is the **only** thing the telegram subsystem reaches into `connection_service` for | re-export, duplicate or re-implement `now_unix_ms` |
| `n1` host-worktree-services | the nine cycle cuts, `config.rs:85` among them | `session_room::DEFAULT_GIT_TIMEOUT` no longer reaches back into the wiring layer, so a subsystem crate can depend on `config` | cut any further cycle; if one surfaces here it is node 1's to cut |
| `n1` host-worktree-services | `run_server(RunServerOptions)` | this PR removes nothing from that struct, but registers its `ServiceEntry`s against the post-node-1 shape | change the options struct's shape |

## Draft PR contract

What lands in this PR's **second commit**, to unblock nothing downstream (no node consumes this one)
but to fix the boundary early so a reviewer and a concurrent green on nodes 3–5 can see it:

- `packages/tddy-model-registry/src/lib.rs`, `packages/tddy-telegram/src/lib.rs` and
  `packages/tddy-screen-sharing/src/lib.rs` declaring each crate's public surface with real
  signatures — the `build_*_entry(...) -> ServiceEntry` constructors and the trait ports
  (`TelegramSender`, `ProviderClient`, `ProviderClientFactory`) — bodies annotated
  `// TODO(model-telegram-screen): implement`.
- Their three `Cargo.toml`s and the workspace `members` entries, so `cargo build --workspace` sees them.
- The failing acceptance tests for all three services and for the absence of `vnc.VncService`.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 2 of 3
**Greenable independently:** **only two of the three crates.** `tddy-model-registry` and
`tddy-screen-sharing` are independent as planned — every test mounts its own crate's service or
injects a double. **`tddy-telegram` is not.** Measurement (below, *The telegram cycle*) shows its
centre — `telegram_session_control` — is an orchestrator of the daemon's session lifecycle, so the
subsystem cannot finish leaving until the session machinery does. That machinery belongs to **nodes
4 and 6-8**, which makes telegram a *successor* of them, not a peer of nodes 3-5. What this node
can green independently is the part with no edge back into the daemon: the transport seam and four
leaf modules. The rest is out of reach on this branch at any effort.
**Concurrent with:** nodes 3 and 5 for the registry and screen-sharing halves. All touch disjoint
subsystems, disjoint protos and disjoint test files, and the only shared file is
`packages/tddy-daemon/src/runtime.rs` — where each node removes its own registrations. That is a
recurring conflict, not a dependency: expect a `runtime.rs` conflict on every cascade and resolve it
by keeping both removals. **Node 4 is no longer a peer for the telegram half** — it is a
prerequisite of it.
**Blocks:** nothing. No node consumes anything this one delivers.

Real dependency edges, as opposed to the branch line — the telegram edge is the one the plan missed:

    n1 → n2, n3, n4, n5      n5 → n6, n7, n8

    n4, n6, n7, n8 → n2's telegram remainder   (discovered on M4, not planned)

## Affected Packages

- **tddy-model-registry** *(new)* — the 13 `model_registry/` modules and their two services
- **tddy-telegram** *(new)* — 4 of the 9 telegram and elicitation modules, plus the transport
  seam of a 5th; the other 5 are deferred to nodes 4 and 6-8
- **tddy-screen-sharing** *(new)* — `screen_sharing_service.rs`, `screen_sharing_vault.rs`
- **tddy-daemon**: **19 modules leave and 2 are deleted** — 152 → 131 source files, 160 → 151 test
  files. (The plan said 27 modules; that counted all nine telegram modules, five of which stayed.)
  There is **no `packages/tddy-daemon/README.md`** in the tree — the plan named a file that does not
  exist. Docs, as moved at wrap:
  - [model-registry.md](../../packages/tddy-model-registry/docs/model-registry.md) — moved to `tddy-model-registry/docs/` ✅
  - [telegram-notifier.md](../../packages/tddy-telegram/docs/telegram-notifier.md), [telegram-github-link.md](../../packages/tddy-telegram/docs/telegram-github-link.md) — moved to `tddy-telegram/docs/` ✅
  - [agent-session-status.md](../../packages/tddy-daemon/docs/agent-session-status.md) — **stays.** It has no model-registry half; see `## Final Checklist`
- **tddy-service**: no proto change. `vnc.proto` is left in place with no server
- **tddy-desktop**: no change expected — it consumes `{config, runtime, supervisor_client, spawn_worker, cli_session_manager}`, none of which moves here. **Outside the CI gate**, so verified locally and stated

## Related Feature Documentation

**None — behaviour-preserving restructure. No PRD.** Following the precedent set by
`docs/dev/1-WIP/2026-09-09-connection-service-split.md`: no RPC coordinate moves, no client migrates,
and no observable behaviour changes, so there is nothing a PRD would state that this changeset does
not. The one product-visible delta is the *removal* of a service that was never reachable, which is
recorded in `## Responsibility` and in the changelog entry at wrap.

## Summary

`model_registry/`, the telegram subsystem and the screen-sharing subsystem move out of `tddy-daemon`
into three crates of their own, keeping the proto services they already have. `vnc_service.rs` and
`vnc_vault.rs` are deleted as unreachable.

## Background

These three are the cheapest subsystems in the daemon to extract, and they were chosen as node 2
precisely so that the stack's second use of `move_module_to_crate` is a low-risk one:

- **`model_registry/` is the cleanest extraction in the crate**: already directory-shaped, **zero**
  outbound `crate::` edges beyond its own directory, **zero** inline test LoC (all 4,618 lines of its
  tests are integration tests in dedicated files), and its dependencies (`sqlx`,
  `agent-client-protocol`, `tddy-acp`) are attributable to it alone and leave with it.
- **The telegram subsystem has no `ConnectionService` surface at all.** Its RPCs are
  `tddy/v1/observer.proto` and `tddy/v1/presenter_intent.proto`, owned by `tddy-service`. It is
  8,036 LoC — the second-largest subsystem in the daemon — reachable for one symbol (`now_unix_ms`),
  and `teloxide` leaves with it.
- **`screen_sharing_service.rs` has zero `connection_service` edges** and 0 inline test LoC.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ℹ ANSWERED — `2026-09-06-stubeligibledaemonsource-is-no-longer-reachable-from-production.md`

Asks whether a stub source is still reachable. Discovery answers the adjacent and larger question:
**`vnc_service.rs` and `vnc_vault.rs` are not reachable at all** — `runtime.rs` registers no
`vnc.VncService`, and the only references anywhere in `packages/` are `lib.rs:111`, the two source
files and their two test suites. This node deletes them, and the entry should be re-read at wrap
against what is left.

### ⚠ DURING — `2026-08-16-models-agents-adjacent-findings.md` and `2026-08-16-models-agents-open-items-at-wrap.md`

Both record open items inside `model_registry/`. This node moves that code without changing it, so
every item survives the move unchanged — the value of recording them is that a reviewer seeing them
in the new crate knows they are inherited, not introduced. Neither is fixed here.

### ⚠ DURING — `2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`

The screen-sharing subsystem is one of the places this bites. The move must not deepen it: no new
LiveKit call is added, and the existing ones move verbatim. Recorded, not fixed here.

## Scope

- [x] **`tddy-model-registry`**: 13 modules and 5 test files moved. The plan said 8 test files; a
      grep for every registry symbol across `packages/tddy-daemon/tests/` finds **six**, and one of
      those — `registry_assistant_as_agent_acceptance.rs` — stays behind, because its subject is
      `ConnectionServiceImpl` resolving an assistant as an `--agent`. `sqlx`,
      `agent-client-protocol` and `tddy-acp` left with it ✅
- [~] **`tddy-telegram`**: **partially blocked, and this is the node's one real finding.**

      **Moved (5):** the *transport seam* of `telegram_notifier.rs` — the `TelegramSender` port,
      `TeloxideSender`, `InMemoryTelegramSender`, `send_telegram_via_teloxide`,
      `send_telegram_with_inline_keyboard` and `send_daemon_lifecycle_message`, 204 lines,
      byte-identical — plus four whole leaf modules with no production edge back into the daemon:
      **`active_elicitation.rs`** (241), **`elicitation.rs`** (208),
      **`telegram_tracked_session.rs`** (316) and **`telegram_github_link.rs`** (330). Across those
      four, **7 lines differ** from the originals and every one is an import path or a doc
      reference: `telegram_github_link.rs:16` `crate::config::DaemonConfig` →
      `tddy_daemon_kernel::config::DaemonConfig` (node 1's relocation), 4 doc links to
      `telegram_notifier` / `telegram_session_control` demoted to plain code spans because this
      crate cannot name them, and the acceptance-test path in `telegram_github_link.rs:3` made
      absolute. `telegram_tracked_session.rs` is **100% identical**. Their 17 inline tests moved
      with them. `hmac`, `sha2` and `subtle` left `tddy-daemon` with the OAuth-state signer;
      `base64` stayed, because `auth.rs` uses it too.

      **Not moved (5):** `telegram_session_control.rs`, `telegram_bot.rs`,
      `telegram_session_subscriber.rs` and `telegram_notifier.rs`'s policy half
      (`TelegramSessionWatcher` and the two elicitation caches) cannot leave without inverting the
      dependency — see **The telegram cycle** below. They go to the nodes that own what they reach:
      **node 4** (the `connection.ConnectionService` families) and **nodes 6-8** (the session,
      spawn and git machinery). `telegram_multi_select_shortcuts.rs` (82) is the one that came
      *unblocked* on this milestone rather than being blocked — its single edge was
      `telegram_notifier::InlineKeyboardRows`, which is now `tddy_telegram::sender`'s — but it was
      outside this step's scope and is the next module to follow.

      `teloxide` did **not** leave `tddy-daemon`: unlike `tddy-screenshare` on M3 it genuinely was
      a dependency (4 telegram modules plus `runtime.rs`), and every module that keeps it there is
      one of the five that stayed. No dedicated test file moved either: all **6** suites that exercise the
      moved modules also touch deferred modules or daemon types, so they stay in `tddy-daemon` and
      keep resolving unchanged through the facade. The 17 inline tests that live *inside* the four
      moved files did move, and run under `cargo test -p tddy-telegram`.
      The plan's counts were wrong on every axis: **9** modules, not 10;
      **7,237** production lines, not 6,835; **1,297** inline test lines, not 1,201;
      **12** dedicated test files totalling **4,892** lines, not 14 files / 4,529
- [x] **`tddy-screen-sharing`**: 2 modules and 2 test files moved. `argon2`, `chacha20poly1305` and
      `rand` — the vault's Argon2id/ChaCha20-Poly1305 primitives — left `tddy-daemon` with it.
      `tddy-screenshare` did **not**: this plan said it was screen sharing's alone, and it is, but
      `tddy-daemon` never depended on it — `packages/tddy-vnc` and `packages/tddy-rdp`, the bridge
      binaries the service *spawns*, are its only consumers. There was nothing to move ✅
- [x] **Delete unreachable VNC**: `vnc_service.rs`, `vnc_vault.rs`, both acceptance suites, the two `lib.rs` entries ✅
- [x] **Wiring**: `models.ModelRegistryService`, `tddy.acp.v1.AcpService` and
      `screen_sharing.ScreenSharingService` register through `build_*_entry`, and
      `packages/tddy-daemon/tests/service_registration_acceptance.rs` pins all three against the
      live roster ✅ **Deferred:** telegram's inbound task stays in `runtime.rs` — there is no
      constructor to move it behind while `TelegramSessionControlHarness` is a daemon type. Goes
      with the five blocked modules to nodes 4 and 6-8
- [x] **File budget**: recorded, not forced. `screen_sharing_service.rs` stayed whole at 2,171 lines (962 production, 1,209 inline host-scope suite) — the reasoning is in `### File budget` below ✅
- [x] **Baseline**: green on CI, which is the authority for whole-workspace health —
      **6463/6463 Rust, 2630/2630 Web**, plus Rust build, arm64 build, workspace clippy, the
      generated-code drift gate and VM checks, all passing on `73ff7d2f`. Local runs were scoped to
      the touched packages, per `AGENTS.md` § Verification ✅
- [x] **Code Quality**: the workspace clippy check and `cargo fmt --check` are both green on CI ✅
- [x] **Documentation**: doc triage executed at wrap — see `## Final Checklist` ✅

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | Files | prod LoC | inline tests | outbound `crate::` edges |
|---|---:|---:|---:|---|
| `model_registry/` | 13 | 3,035 | **0** | **none beyond its own directory** |
| telegram | 9 | 7,237 | 1,297 | SESSIONS (6 modules), LIVEKIT (`session_room`), GIT (2), SPAWN (4), MISC (1), CORE (`config`) — **14 of these are still `tddy-daemon`'s own**, which is what blocks the move |
| screen sharing | 2 | 1,367 | **1,209** | AUTH (`screen_sharing_vault`), CORE (`config`), HOSTS (`host_desktop_targets`, `host_keypair`, `host_prompts`, `host_registry`) — **no `connection_service` edge** |
| vnc | 2 | 563 | 0 | **unreachable — registered nowhere** |

`teloxide` is used by the telegram subsystem alone; `sqlx`, `agent-client-protocol` and `tddy-acp` by
`model_registry/` alone; `argon2`, `chacha20poly1305` and `rand` by screen sharing alone.

The screen-sharing row above was corrected against the code at M3; its totals had been read off
the file sizes without splitting production from tests. `screen_sharing_service.rs` is 2,171 lines,
of which **1,209 are a `#[cfg(all(test, unix))]` module** — the `#hosts-screen` host-scope suite — so the
subsystem is 1,367 production lines, not 2,299, and its inline-test count is 1,209, not 0. Its
outbound edges were understated too: it reaches four `host_*` modules as well as `config`. All four
had already left `tddy-daemon` on node 1, so the extraction was still one-way — but that is node 1's
doing, not an absence in the subsystem. `tddy-screenshare` was never a `tddy-daemon` dependency;
see `## Scope`.

### State B

`tddy-daemon` loses 27 modules and 12,169 prod LoC, and deletes 2 more. Three new crates each expose
a `build_*_entry(...) -> tddy_rpc::ServiceEntry` plus the trait ports their hosts inject
(`TelegramSender`, `ProviderClient`, `ProviderClientFactory`, `ChatWorkspaceRoots`, `SessionsBase`).
`runtime.rs` registers them by calling those constructors instead of naming daemon-internal types.

### Delta

#### tddy-daemon
- **Architecture**: 27 modules leave, 2 are deleted; `lib.rs` loses 29 entries
- **Implementation**: `runtime.rs`'s `models.ModelRegistryService`, `AcpService`,
  `screen_sharing.ScreenSharingService` registrations and the telegram inbound task move behind the
  new crates' constructors
- **Dependencies**: `teloxide`, `sqlx`, `agent-client-protocol`, `tddy-acp`, `argon2`,
  `chacha20poly1305`, `rand` leave. Not `tddy-screenshare` — it was never a dependency of this crate

#### tddy-model-registry, tddy-telegram, tddy-screen-sharing
- **Architecture**: new crates, each a workspace member
- **API**: one entry constructor plus the existing trait ports, unchanged in shape

### The published surface vs. the code, at M3

`tddy-screen-sharing/src/lib.rs` was published before the subsystem was read, per the draft-PR
contract, and contradicted it in four places. The code is the truth and the surface was corrected to
it — never the reverse.

| Published stub | The code | Resolution |
|---|---|---|
| `build_screen_sharing_entry(SessionsBase, Arc<ScreenSharingVault>)` | the vault is **per session, on disk**, opened inside a call from the session's own directory — it is never injected, and one process holds many | takes the assembled `ScreenSharingServiceImpl`. `with_config` and `with_host_scope` are genuinely optional, and flattening the builder would make two optional collaborators required |
| `ScreenSharingVault { seal(session_id, key), unseal(session_id) -> Option<Vec<u8>> }` | an Argon2id-keyed **credential** vault: `create`/`unlock` a `.screen-sharing.yaml`, `add_target`, `list_targets`, `remove_target`, `decrypt_password` | the real vault, re-exported. The three vault tests keep their names and their intent, restated against it |
| `ScreenSharingError::Unsealable { .. }` | no such type. The vault returns `anyhow::Result`; the service returns `tddy_rpc::Status` | deleted. Inventing an error type with no raiser would have been new behaviour in a relocation |
| `pub type SessionsBase` declared afresh | `screen_sharing_service::SessionsBase`, character-for-character the same alias | re-exported rather than duplicated |

The one thing the stub got right is the one that mattered most on M1: the registered name.
`screen_sharing.proto` declares `package screen_sharing;` over `service ScreenSharingService`, so
the wire coordinate **is** `screen_sharing.ScreenSharingService` and `runtime.rs` was already
registering it correctly. Unlike `acp.AcpService` on M1, nothing moved.

### The telegram cycle — why M4 is 1,299 lines instead of 7,237

The plan assumed telegram was extractable because it "reached the god module for exactly one
symbol", `now_unix_ms`. That is true and irrelevant. The blocker was never `connection_service`;
it is the daemon's **session machinery**, which the plan listed in State A (`SESSIONS (6 modules),
LIVEKIT, GIT (2), SPAWN (4), MISC (1)`) and then did not check the ownership of. On M3 the
equivalent list turned out to have already left for `tddy-host-service`. Here it has not: 14 of
the 16 targets are still `tddy-daemon`'s own modules, and they belong to nodes 4 and 6–8.

Measured against the code, non-doc production edges only — `crate::` references inside `///`
comments were counted as edges by the initial discovery and are not:

| Module | prod LoC | Real outbound production edges | Outcome |
|---|---:|---|---|
| `active_elicitation` | 155 | **none** (all four were doc links) | ✅ moved |
| `elicitation` | 180 | **none** (doc link only) | ✅ moved |
| `telegram_tracked_session` | 210 | **none** (doc link only) | ✅ moved, byte-identical |
| `telegram_github_link` | 275 | `config` — node 1's kernel re-export | ✅ moved, 2 lines re-pointed |
| `telegram_notifier` | 1,442 | `active_elicitation`, `elicitation`, `telegram_multi_select_shortcuts`, `telegram_tracked_session`, `config`, **`telegram_session_control`** | ⚠ transport seam only; watcher → nodes 4, 6-8 |
| `telegram_multi_select_shortcuts` | 82 | `telegram_notifier` — and only for `InlineKeyboardRows`, which is now `tddy_telegram::sender`'s | ⚠ **now unblocked**, out of this step's scope |
| `telegram_session_subscriber` | 129 | `config`, `telegram_notifier`, **`session_notifications`** | ❌ → nodes 6-8 |
| `telegram_bot` | 788 | `telegram_notifier`, `telegram_tracked_session`, **`telegram_session_control`** | ❌ → follows `telegram_session_control` |
| `telegram_session_control` | 3,976 | **12 daemon-owned modules** plus `branch_owner`/`project_storage` (node 1's) | ❌ → nodes 4 and 6-8 |

The prod-LoC column is the initial discovery's count of non-comment production lines; the four
moved files are 241, 208, 316 and 330 lines on disk including doc comments and inline tests.

`telegram_session_control.rs` is not a chat adapter that happens to be large. It is an orchestrator
of the daemon's session lifecycle: it holds `cli_session_manager::CliSessionManager` and
`session_room::SessionRoomRegistry` as struct fields, dispatches through `supervisor_client` /
`supervisor_spawn` / `spawn_worker` / `spawner`, reads through `session_reader`, deletes through
`session_deletion`, and renders through `session_list_enrichment`. It cannot precede the session
subsystem out of the crate; it has to follow it.

Two of those edges close a **cycle**, so they would not be fixed even by moving the whole telegram
subsystem at once. Both are real production `use`/call sites, verified at `file:line` and not doc
links:

    session_list_enrichment.rs:305       → crate::elicitation::pending_elicitation_for_session_dir
                                                                     (daemon → telegram)
    telegram_session_control.rs:33,673   → crate::session_list_enrichment::{SessionListStatusDisplay,
                                             session_list_status_from_session_dir}
                                                                     (telegram → daemon)

    session_notifications.rs:366         → crate::elicitation::mode_changed_requires_telegram_elicitation
                                                                     (daemon → telegram)
    telegram_session_subscriber.rs:14    → crate::session_notifications::{...}
                                                                     (telegram → daemon)

The plan's `## Decisions & Trade-offs` states "the session modules consume `tddy-telegram` rather
than the reverse — so this creates no cycle". The first half is right and the second does not
follow: the reverse edges exist too, and Cargo cannot express a mutual pair across crates.

**How the four moves stay cycle-free anyway.** The cut is not between telegram and the daemon; it
runs *through* the telegram subsystem. `elicitation` moved and its two daemon callers stayed, so the
forward edge became `tddy-daemon` → `tddy_telegram::elicitation` — one-way, and exactly the direction
every other facade in `lib.rs` already points. The two modules holding the reverse edges,
`telegram_session_control` and `telegram_session_subscriber`, stayed *with* those callers, so their
edges back are intra-daemon and Cargo never sees them. Splitting the subsystem at that line is what
made four of nine modules movable at all; moving all nine, as planned, is the one arrangement that
cannot compile.

**What did move.** `telegram_notifier.rs` splits cleanly at the line between *transport* (how to
put bytes on the Bot API) and *policy* (`TelegramSessionWatcher`: when to). The transport half
depends only on `teloxide`, `async_trait` and `DaemonConfig`; it does not touch
`chunk_telegram_text` or `CB_ENTER`, the two `telegram_session_control` imports, which are the
watcher's. That made it liftable, and it is a seam the file budget wanted split anyway. The four
leaf modules then followed it, for a total of 1,299 lines in `tddy-telegram`.

**The facade is what keeps the caller diff at zero.** `packages/tddy-daemon/src/lib.rs` trades each
`pub mod <m>;` for `pub use tddy_telegram::<m>;`, so `crate::active_elicitation::X` inside the daemon
and `tddy_daemon::telegram_tracked_session::Y` in its acceptance suites both go on resolving. **6
daemon source files and 6 daemon test files reference the four moved modules, and not one of them
changed** — `runtime.rs`, `session_list_enrichment.rs`, `session_notifications.rs`,
`telegram_bot.rs`, `telegram_notifier.rs`, `telegram_session_control.rs`; and
`session_notifications_acceptance.rs`, `telegram_branch_conflict_acceptance.rs`,
`telegram_claude_cli_activity_alert_acceptance.rs`, `telegram_github_link.rs`,
`telegram_notification_subscriber_unit.rs`, `telegram_tracked_session_acceptance.rs`.
`cargo build -p tddy-daemon` is clean. This is the same mechanism node 1 used for
`config` (`lib.rs:23`) and the eight worktree modules (`lib.rs:13`).

**What this still costs the node.** `teloxide` stays in `tddy-daemon`, no dedicated test file moves,
and `runtime.rs`'s telegram inbound task stays where it is — there is no constructor to move it
behind while `TelegramSessionControlHarness` is still a daemon type. The telegram remainder should be
re-planned as a *successor* of nodes 4 and 6–8 rather than a peer of nodes 3–5.

### The published surface vs. the code, at M4

`tddy-telegram/src/lib.rs` was published before the subsystem was read, and four of its five
declarations were fiction.

| Published stub | The code | Resolution |
|---|---|---|
| `trait TelegramSender { fn send(&self, text) -> Result<(), TelegramError> }` | `#[async_trait] trait TelegramSender { async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()>; async fn send_message_with_keyboard(&self, chat_id, text, InlineKeyboardRows) }` — async, two methods, addressed per chat, `anyhow` | the real trait, moved. A one-method sync `send` would have dropped the keyboard surface every elicitation depends on |
| `enum TelegramError { NotConfigured, Rejected }` | **no such type anywhere.** Every telegram path returns `anyhow::Result`; "not configured" is not an error at all but an early `Ok(())` | deleted, and `thiserror` with it. Same call as `ScreenSharingError::Unsealable` on M3: an error type with no raiser is new behaviour, not a relocation |
| `enum LifecycleEvent { Started, Stopped }` | the real function takes `text: &str`. The wording is the **caller's**: `server.rs:68` composes `format!("tddy-daemon started ({instance_id})")`, and the shutdown path passes the literal `"tddy-daemon stopped"` | deleted. The instance id in the real message has nowhere to live in a two-variant enum |
| `send_daemon_lifecycle_message(&dyn TelegramSender, LifecycleEvent)` | `send_daemon_lifecycle_message<S: TelegramSender + ?Sized>(config: &DaemonConfig, sender: &S, text: &str)` — reads `config.telegram`, returns `Ok(())` when absent or `!enabled`, and otherwise **fans out to every `chat_ids` entry** | the real signature. The stub had no config parameter, so it could express neither the gate nor the fan-out — the two things this function exists for |
| `TelegramDaemonHooks::new(Option<Arc<dyn TelegramSender>>)` | a 3-field struct `{ config, sender, watcher }` in `telegram_session_subscriber` with a **required** sender and **no constructor**; optionality lives at the call site as `Option<Arc<TelegramDaemonHooks>>`, produced by `runtime.rs`'s `build_telegram` | not published — the module is on the blocked side of the cycle. The stub also inverted the design: "unconfigured" is the *absence of the whole hooks value*, never a hooks value holding `None` |

The two failing tests were restated against the real behaviour, keeping their intent:

| Test | Change | Why |
|---|---|---|
| `announces_that_the_daemon_started` | kept its name; now supplies a `DaemonConfig` with `telegram.enabled` and one chat id, passes the announcement text `server.rs` really composes, and additionally asserts it reached the configured chat | the stub called a two-argument function that does not exist. The "started" assertion is unchanged |
| `builds_hooks_that_carry_no_sender_when_telegram_is_unconfigured` → `delivers_nothing_when_telegram_is_unconfigured` | renamed, because `TelegramDaemonHooks::new` is fiction and the type cannot move. Its stated intent — "not an error the daemon should fail to start over, but not a silent success either" — is asserted verbatim against `send_daemon_lifecycle_message` with no `telegram:` block: `Ok(())`, nothing delivered | the intent survives; only the subject that could carry it changed |
| *(added)* `delivers_nothing_when_telegram_is_configured_but_disabled` | new | `enabled: false` is a third state the real function branches on separately, and it is how an operator mutes the bot without deleting the token. The stub surface had no way to express it |

### File budget

`screen_sharing_service.rs` landed at **2,171 lines — over budget, unsplit**, and this node does not
split it. 1,209 of those lines are the `#[cfg(all(test, unix))]` host-scope suite and 962 are
production, so the production file is already inside two budgets' worth of the limit while the
*file* is not. Splitting it would mean either cutting the seam between session-scoped and
host-scoped calls — which is a design change, and `#hosts-screen 8/8` deliberately put both on one
implementation so the bridge and the LiveKit republishing are shared rather than duplicated — or
lifting the inline suite into `tests/`, which would cost it access to the private helpers it
asserts on. Recorded, not forced.

## Implementation Milestones

- [x] M1 — `tddy-model-registry` extracted; its 5 test files pass in the new crate (134 tests: 3 crate-level + 131 across the five moved suites) ✅
- [x] M2 — the unreachable VNC service and its suites deleted; nothing references them. The
      inverse assertion lives in `tddy-screen-sharing/tests/dead_vnc_service_removed.rs` (3 tests) ✅
- [x] M3 — `tddy-screen-sharing` extracted; its 2 suites pass (39 tests: 21 crate-level — 4
      surface + 17 inline host-scope — plus 3 VNC-absence, 6 service acceptance, 9 vault
      acceptance) ✅
- [~] M4 — **partially achievable only.** The transport seam plus 4 leaf modules extracted
      (**20 tests pass** in `tddy-telegram`: 3 crate-level surface + 17 inline moved with the
      modules, up from 3). The remaining 5 modules and `teloxide` are blocked on a production
      cycle and deferred to nodes 4 and 6-8 ⚠
- [x] M5 — `runtime.rs` registers all three through their constructors and CI is green
      (6463/6463 Rust, 2630/2630 Web) ✅ The telegram inbound task is the one piece not behind a
      constructor, deferred with the five blocked modules
- [x] M6 — file-budget outcome recorded: one file over, unsplit, with its reason ✅

## Testing Plan

**Primary test level: integration, per package.** Every deliverable is a boundary change, and the
tests that prove it already exist — 9,771 LoC of them, in dedicated files, organised per subsystem by
name. They **move with the code** rather than being rewritten, which is the strongest available
evidence that behaviour did not change.

Two mechanisms carry the proof that a test cannot:

- **`restructure verify --against <pre-move ref>`**, run from the repo root, compares statement
  multisets repo-wide and does not compare paths — so it validates a cross-crate move directly.
- **The moved-line diff**: normalise `pub(crate)` and whitespace away, set-compare every moved line
  against the pre-move ref, and state how many lines differ and why each one does. Run alongside the
  visibility table, never instead of it — normalising `pub(crate)` away is exactly what hides those rows.

For the deletion, the assertion is the inverse: a test that `vnc.VncService` is **not** among the
daemon's registered service names, so the removal cannot silently regress into a re-registration.

## Acceptance Tests

### tddy-model-registry
- [x] **Integration**: all 12 `models.ModelRegistryService` methods answer from the new crate (`packages/tddy-model-registry/tests/model_registry_service_acceptance.rs`) ✅
- [x] **Integration**: the model-addressed ACP service answers from the new crate (`packages/tddy-model-registry/tests/model_acp_service_acceptance.rs`). Registered as **`tddy.acp.v1.AcpService`** — `acp.AcpService` elsewhere in this document is prose shorthand for the same coordinate ✅
- [x] **Unit**: the store opens, migrates and round-trips without `tddy-daemon` on the dependency path (`packages/tddy-model-registry/tests/model_registry_store_unit.rs`) ✅

### tddy-telegram
- [~] **Integration**: a session started from telegram reaches the daemon and reports back (`telegram_start_claude_acceptance.rs`). **The suite passes, unchanged, but it did not move** — its subject is `telegram_session_control` / `telegram_bot`, both blocked on the daemon's session machinery. It exercises `tddy-daemon`, not `tddy-telegram`. **Deferred to nodes 4 and 6-8**, with the modules
- [~] **Integration**: concurrent elicitations resolve independently (`telegram_concurrent_elicitation_integration.rs`). Same: passes unchanged in `tddy-daemon`, reaches `tddy_telegram::active_elicitation` only through the daemon's facade. **Deferred to nodes 4 and 6-8**
- [x] **Unit**: the notifier composes a lifecycle message without `tddy-daemon` on the dependency path ✅ — asserted in `packages/tddy-telegram/src/lib.rs`'s test module against `send_daemon_lifecycle_message`, over three states (configured, unconfigured, configured-but-disabled). Not in a file called `telegram_notifier.rs`: only the transport half moved, and it landed as `sender.rs`

### tddy-screen-sharing
- [x] **Integration**: all 10 `screen_sharing.ScreenSharingService` methods answer from the new crate (`packages/tddy-screen-sharing/tests/screen_sharing_service_acceptance.rs`) ✅
- [x] **Integration**: the vault seals and unseals a key from the new crate (`packages/tddy-screen-sharing/tests/screen_sharing_vault_acceptance.rs`) ✅

### tddy-daemon
- [x] **Integration**: the daemon's registered service names include the three moved services and **not** `vnc.VncService` (`packages/tddy-daemon/tests/service_registration_acceptance.rs`) ✅ Written at wrap — it was the one planned criterion with no test. It builds a real runtime through `runtime::build(config, RuntimeOptions::for_embedded())` and asserts on `DaemonRuntime::service_names()`. The config carries `github.stub: true`, because every one of these services is registered inside `runtime::build`'s `if let Some(user_resolver)` block and a config without GitHub auth would make all four assertions vacuous; the VNC test guards against exactly that by first asserting the auth-gated roster is present

## Decisions & Trade-offs

- **Three unrelated crates in one PR.** `tddy-model-registry` and `tddy-telegram` have nothing to do
  with each other; they share a node only because both are pure relocations. This was a deliberate
  consolidation from a 25-node plan down to 8, accepting a larger diff in exchange for fewer PRs to
  land. The mitigation is that the three are reviewable independently *within* the diff — they touch
  disjoint files — and that `verify --against` plus the moved-line diff make a mechanical move
  reviewable without reading every line. If a reviewer will not accept that contract, this node
  should be split back into three.
- **The VNC deletion rides along rather than getting its own node.** It is 1,008 lines of provably
  unreachable code sitting beside the live screen-sharing service, and this is the only node whose
  reviewer is already reading that subsystem. The alternative — a dedicated cleanup node — would ask
  someone to review a deletion with no surrounding context.
- **`vnc.proto` is not deleted.** Removing the service is provable from `runtime.rs`; removing the
  schema is a judgement about whether anyone intends to revive it, and that is not this node's call.
  Recorded in `docs/dev/todo/` at wrap.
- **The elicitation modules go with telegram, and their callers stay.** `active_elicitation.rs` and
  `elicitation.rs` are telegram's behaviour, but `session_list_enrichment.rs` and
  `session_notifications.rs` reach them and belong to later nodes. **The first half of this held and
  the conclusion did not.** The forward direction is correct, and both those callers now do consume
  `tddy_telegram::elicitation` with no caller diff at all. But the plan concluded from that "so this
  creates no cycle", and the *reverse* edges exist too — measured, at `file:line`:

      session_list_enrichment.rs:305       → crate::elicitation::pending_elicitation_for_session_dir
      telegram_session_control.rs:33,673   → crate::session_list_enrichment::{SessionListStatusDisplay,
                                              session_list_status_from_session_dir}

      session_notifications.rs:366         → crate::elicitation::mode_changed_requires_telegram_elicitation
      telegram_session_subscriber.rs:14    → crate::session_notifications::{...}

  Cargo cannot express a mutual pair across two crates, so the cycle is only cut by keeping *both*
  reverse-edge modules inside `tddy-daemon` — which is what happened. `elicitation` moved and its two
  daemon callers stayed, giving a one-way `tddy-daemon` → `tddy-telegram` edge;
  `telegram_session_control` and `telegram_session_subscriber` stayed with them, so their edges back
  are intra-daemon and invisible to Cargo. Moving the whole subsystem at once, as the plan intended,
  would have been the one arrangement that *cannot* compile.

## Technical Debt & Production Readiness

Every box below is **deliberately unticked at wrap** — each is a known, deferred item with an owner,
not an unfinished part of this node.

- [ ] **Deferred, no owner yet — not this stack's.** `tddy-service` depends on `tddy-tui`, so any subsystem crate that needs `tddy-service`'s protos
      pulls the TUI into its build. Not introduced here, but each new crate inherits it
- [ ] **Deferred to a product call in its own PR.** `vnc.proto` is left with no server, and the wrap
      triage found that `tddy-web`'s session inspector still has a user-reachable "vnc" tab dialling
      it. Retiring the proto therefore means removing a UI tab, which is product-visible and cannot
      ride a relocation node. Recorded in
      [`docs/dev/todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md`](../todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md)
- [ ] **Deferred to a deliberate rename step with a migration note; no node owns it yet.** Four
      `log::` calls in the moved registry still name `target: "tddy_daemon::model_registry"`
      (`error.rs:103`, `service.rs:197`, `acp_service.rs:265,628`). Left verbatim on purpose: a log
      target is an operator-facing filter, and renaming it is an observable change, not a
      relocation. Re-point them to `tddy_model_registry` as a deliberate step, with the same done
      for telegram and screen sharing
- [ ] **Deferred — a one-line test strengthening, written and then withdrawn unverified.**
      `tddy-screen-sharing`'s `does_not_return_one_sessions_key_to_another` proves cross-session
      isolation only through `vault_b.list_targets().is_empty()`. Its name promises more than that:
      the property is that session B's *key* cannot open session A's sealed password, and the empty
      listing is evidence for it rather than the thing itself. One extra assertion says it directly:

          assert!(vault_b.decrypt_password(&as_target.id, &key_b).is_err(),
                  "one session's key must not open another session's sealed password");

      Not applied at wrap because it could not be verified locally — the worktree was under a
      concurrent full `cargo test -p tddy-daemon` from another session and free disk was at the
      15Gi stop line, so an unverifiable edit to a CI-green test was the worse trade. The existing
      assertion is real, not vacuous; this is a sharpening, not a fix

- [ ] **Deferred with the item above, and for the same reason.** In `tddy-telegram`: **40** `log::` calls across the five moved files still name
      `tddy_daemon::…` — `tddy_daemon::active_elicitation` (6), `tddy_daemon::elicitation` (9),
      `tddy_daemon::telegram` (8), `tddy_daemon::telegram_github_link` (13) and 4 in `sender.rs`.
      One is not a call at all but a **public constant**,
      `telegram_tracked_session::TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET =
      "tddy_daemon::telegram_bot::message-body"`, which operators match on in `daemon.yaml` `log:`
      policies and one of the moved inline tests asserts the suffix of. Renaming any of them
      silently breaks a deployed log filter, so all 40 stay verbatim until it is a deliberate step
      with a migration note

## Baseline

Recorded before any change.

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | not re-run locally — a whole-package daemon run is CI's; `cargo build -p tddy-daemon` is clean |
| `cargo test -p tddy-telegram` | 2 failing / 0 passing (the published stubs) | **20 passed / 0 failed** — 3 crate-level surface + 17 inline, moved with the four modules |
| `cargo clippy -p tddy-model-registry -p tddy-telegram -p tddy-screen-sharing --all-targets -- -D warnings` | ✅ exit 0 | `-p tddy-telegram --all-targets` ✅ exit 0; `cargo fmt --check` ✅ exit 0 |
| `cargo test -p tddy-daemon --test service_registration_acceptance` | ⛔ the suite did not exist | **4 passed / 0 failed** (written at wrap; `cargo check --tests -p tddy-daemon` also exit 0) |
| **CI on `73ff7d2f`** — the authority for whole-workspace health | n/a | ✅ **6463/6463 Rust, 2630/2630 Web**, Rust build, arm64 build, workspace clippy, generated-code drift gate, VM checks |

**Whole-workspace green is CI's claim, not a local one.** Local runs here were scoped to the
packages this node touches, per `AGENTS.md` § Verification.

**12 failing tests** defined this node: 3 in `tddy-model-registry`, 2 in `tddy-telegram`, 4 in
`tddy-screen-sharing`, and 3 asserting the unreachable VNC service's four files are gone. That last
set is asserted against the daemon's **source** rather than its service registry, because
`vnc.VncService` is already absent from the registry — which is precisely the problem: the module is
declared, compiled, tested, and reachable by nothing.

A **13th** was added at wrap and now passes with them:
`packages/tddy-daemon/tests/service_registration_acceptance.rs` (4 tests) asserts the *live* roster —
the two model-registry entries and the screen-sharing entry are registered, `vnc.VncService` is not.
The source assertion and the registry assertion are complementary rather than redundant: the source
one catches a resurrected file, the registry one catches a resurrected `rpc_entries.push`, and only
the registry one would have caught a moved service failing to re-register at all.

The one known pre-existing failure
(`cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`,
`self_arc called before set_self_handle`) is inherited from node 1's baseline and is expected to stay
at exactly one. Verification is scoped to the packages this node touches.

## Final Checklist

- [x] `docs/dev/changesets/2026-09-09-unbundle-model-telegram-screen.md` — written: the deleted-VNC
      decision (including the live web client it revealed), the 152 → 131 / 160 → 151 before-and-after
      counts, and the telegram partial delivery with the measured cycle behind it ✅
- [x] Moved with `git mv`: `model-registry.md` → `packages/tddy-model-registry/docs/`,
      `telegram-notifier.md` and `telegram-github-link.md` → `packages/tddy-telegram/docs/`.
      Their relative links were re-pointed, and `telegram-notifier.md` gained a "where this code
      lives" header table, because only its transport half moved ✅
- [~] `packages/tddy-daemon/docs/agent-session-status.md` — **not applicable, verified.** The
      document has **no model-registry half**: it is entirely about
      `session_agent_inference`, the agent that *is* a claude-cli/cursor session, and its only
      registry-adjacent line is one sentence distinguishing it from the agent roster. Nothing to
      split, and the document stays in `tddy-daemon` where its subject does. The plan's
      `## Affected Packages` line was wrong about it
- [x] New `docs/dev/todo/` entry:
      [`2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md`](../todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md).
      It records more than the plan expected: the schema has no server **and** `tddy-web`'s session
      inspector still has a user-reachable "vnc" tab dialling it ✅
- [x] Re-read `2026-09-06-stubeligibledaemonsource-is-no-longer-reachable-from-production.md` —
      **still open, unchanged.** Same three test callers; the type moved to `tddy-host-service` on
      node 1, so the fix is now cross-crate. A dated re-read section was appended to that entry ✅
- [x] Doc triage executed. `packages/tddy-daemon/README.md` **does not exist** — the plan's
      `## Affected Packages` names a file that is not in the tree. Re-pointed: two links in
      `packages/tddy-daemon/docs/connection-service.md`, one in `session-notifications.md`, three in
      `docs/ft/daemon/telegram-notifications.md`, one in `docs/ft/daemon/telegram-session-control.md`,
      plus three module names now naming their new crate (`tddy_telegram::elicitation`,
      `tddy_telegram::telegram_github_link`, `tddy_telegram::sender`), each noting the daemon
      re-export that keeps the old path resolving. Historical entries under `docs/dev/changesets/`,
      `docs/ft/daemon/changelog/` and `packages/tddy-daemon/docs/changesets/` were **left alone**:
      they record the state at the time they were written ✅
