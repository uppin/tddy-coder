# Changeset: model registry, telegram and screen sharing in their own crates

**Date**: 2026-09-09
**Status**: 🚧 In Progress
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
| `tddy-telegram` | 10 | 6,835 | nothing — `PresenterObserver` / `PresenterIntent` are `tddy-service`'s | 14 files / 4,529 LoC |
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
- Force every file under 500 lines. `telegram_session_control.rs` (4,158), `telegram_notifier.rs`
  (1,846), `screen_sharing_service.rs` (1,953), `model_registry/store.rs` (1,075), `telegram_bot.rs`
  (737) and `model_registry/acp_service.rs` (617) are all over budget and are split where the seams
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
**Greenable independently:** yes — once node 1 is green. Every test here mounts its own crate's
service or injects a double; none exercises a sibling node's behaviour.
**Concurrent with:** nodes 3, 4 and 5. All four touch disjoint subsystems, disjoint protos and
disjoint test files, and the only shared file is `packages/tddy-daemon/src/runtime.rs` — where each
node removes its own registrations. That is a recurring conflict, not a dependency: expect a
`runtime.rs` conflict on every cascade and resolve it by keeping both removals.
**Blocks:** nothing. No node consumes anything this one delivers.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n5 → n6, n7, n8

## Affected Packages

- **tddy-model-registry** *(new)* — the 13 `model_registry/` modules and their two services
- **tddy-telegram** *(new)* — the 10 telegram and elicitation modules
- **tddy-screen-sharing** *(new)* — `screen_sharing_service.rs`, `screen_sharing_vault.rs`
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 27 modules leave, 2 are deleted
  - [model-registry.md](../../packages/tddy-daemon/docs/model-registry.md) → `tddy-model-registry/docs/`
  - [telegram-notifier.md](../../packages/tddy-daemon/docs/telegram-notifier.md), [telegram-github-link.md](../../packages/tddy-daemon/docs/telegram-github-link.md) → `tddy-telegram/docs/`
  - [agent-session-status.md](../../packages/tddy-daemon/docs/agent-session-status.md) — its model-registry half moves
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
- [~] **`tddy-telegram`**: crate and surface published; 10 modules, 14 test files and `teloxide` still to move
- [x] **`tddy-screen-sharing`**: 2 modules and 2 test files moved. `argon2`, `chacha20poly1305` and
      `rand` — the vault's Argon2id/ChaCha20-Poly1305 primitives — left `tddy-daemon` with it.
      `tddy-screenshare` did **not**: this plan said it was screen sharing's alone, and it is, but
      `tddy-daemon` never depended on it — `packages/tddy-vnc` and `packages/tddy-rdp`, the bridge
      binaries the service *spawns*, are its only consumers. There was nothing to move ✅
- [x] **Delete unreachable VNC**: `vnc_service.rs`, `vnc_vault.rs`, both acceptance suites, the two `lib.rs` entries ✅
- [~] **Wiring**: `models.ModelRegistryService`, `tddy.acp.v1.AcpService` and
      `screen_sharing.ScreenSharingService` now register through `build_*_entry`; telegram's
      inbound task is M4's
- [~] **File budget**: `screen_sharing_service.rs` stayed whole at 2,171 lines — see below
- [ ] **Baseline**: `./test -p tddy-daemon -p tddy-model-registry -p tddy-telegram -p tddy-screen-sharing` back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | Files | prod LoC | inline tests | outbound `crate::` edges |
|---|---:|---:|---:|---|
| `model_registry/` | 13 | 3,035 | **0** | **none beyond its own directory** |
| telegram | 10 | 6,835 | 1,201 | SESSIONS (6 modules), LIVEKIT (`session_room`), GIT (2), SPAWN (4), MISC (1), CORE (`config`) |
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
- [ ] M4 — `tddy-telegram` extracted; its 14 suites pass; `teloxide` gone from `tddy-daemon`
- [ ] M5 — `runtime.rs` registers all three through their constructors; baselines restored
- [ ] M6 — file-budget outcome recorded

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
- [ ] **Integration**: all 12 `models.ModelRegistryService` methods answer from the new crate (`model_registry_service_acceptance.rs`)
- [ ] **Integration**: `acp.AcpService` answers from the new crate (`model_acp_service_acceptance.rs`)
- [ ] **Unit**: the store opens, migrates and round-trips without `tddy-daemon` on the dependency path (`model_registry_store_unit.rs`)

### tddy-telegram
- [ ] **Integration**: a session started from telegram reaches the daemon and reports back (`telegram_start_claude_acceptance.rs`)
- [ ] **Integration**: concurrent elicitations resolve independently (`telegram_concurrent_elicitation_integration.rs`)
- [ ] **Unit**: the notifier composes a lifecycle message without `tddy-daemon` on the dependency path (`telegram_notifier.rs`)

### tddy-screen-sharing
- [ ] **Integration**: all 10 `screen_sharing.ScreenSharingService` methods answer from the new crate (`screen_sharing_service_acceptance.rs`)
- [ ] **Integration**: the vault seals and unseals a key from the new crate (`screen_sharing_vault_acceptance.rs`)

### tddy-daemon
- [ ] **Integration**: the daemon's registered service names include the three moved services and **not** `vnc.VncService` (`service_registration_acceptance.rs`)

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
  `session_notifications.rs` reach them and belong to later nodes. The direction is correct — the
  session modules consume `tddy-telegram` rather than the reverse — so this creates no cycle.

## Technical Debt & Production Readiness

- [ ] `tddy-service` depends on `tddy-tui`, so any subsystem crate that needs `tddy-service`'s protos
      pulls the TUI into its build. Not introduced here, but each new crate inherits it
- [ ] `vnc.proto` is left with no server; the decision to retire it is deferred to `docs/dev/todo/`
- [ ] Four `log::` calls in the moved registry still name `target: "tddy_daemon::model_registry"`
      (`error.rs:103`, `service.rs:197`, `acp_service.rs:265,628`). Left verbatim on purpose: a log
      target is an operator-facing filter, and renaming it is an observable change, not a
      relocation. Re-point them to `tddy_model_registry` as a deliberate step, with the same done
      for telegram and screen sharing

## Baseline

Recorded before any change.

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | |
| `cargo clippy -p tddy-model-registry -p tddy-telegram -p tddy-screen-sharing --all-targets -- -D warnings` | ✅ exit 0 | |

**12 failing tests** define this node: 3 in `tddy-model-registry`, 2 in `tddy-telegram`, 4 in
`tddy-screen-sharing`, and 3 asserting the unreachable VNC service's four files are gone. That last
set is asserted against the daemon's **source** rather than its service registry, because
`vnc.VncService` is already absent from the registry — which is precisely the problem: the module is
declared, compiled, tested, and reachable by nothing.

The one known pre-existing failure
(`cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`,
`self_arc called before set_self_handle`) is inherited from node 1's baseline and is expected to stay
at exactly one. Verification is scoped to the packages this node touches.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-model-telegram-screen.md` — the release-note file,
      carrying the deleted-VNC decision and the before/after module counts
- [ ] Move `model-registry.md`, `telegram-notifier.md` and `telegram-github-link.md` to the new
      packages' `docs/`
- [ ] `packages/tddy-daemon/docs/agent-session-status.md` — split its model-registry half out
- [ ] New `docs/dev/todo/` entry: whether to retire `vnc.proto` now that it has no server
- [ ] Re-read `docs/dev/todo/2026-09-06-stubeligibledaemonsource-is-no-longer-reachable-from-production.md`
      against what is left
- [ ] Doc triage: `grep -rn -e 'model_registry' -e 'telegram' -e 'screen_sharing' -e 'vnc' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon`
