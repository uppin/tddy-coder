# Changeset: the auth and LiveKit subsystems in their own crates

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **4 of 8**. PR [#473](https://github.com/uppin/tddy-coder/pull/473).
Base: `feature/unbundle/sandbox-spawn-services` (node 3, PR #472)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-auth-livekit-initial-discovery.md](./2026-09-09-unbundle-auth-livekit-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

| New crate | Modules | prod LoC | Serves | Dedicated tests |
|---|---:|---:|---|---|
| `tddy-daemon-auth` | 7 | 2,145 | `auth.AuthService` (5), `auth.LiveKitTokenService` (1), `token.TokenService` (2), `loopback_tunnel.LoopbackTunnelService` (1) — **all already their own protos** | 2 files / 194 LoC + 1,862 inline |
| `tddy-daemon-livekit` | 5 | 6,642 | **`livekit.LiveKitService` (1, new)** — family T | 18 files / 6,898 LoC |

`tddy-daemon-auth` owns `build_auth_entries(...) -> AuthBuildResult`, whose `user_resolver` is what
every other service in the daemon authenticates with. This crate is the daemon's identity boundary.

`tddy-daemon-livekit` keeps the four trait ports `session_room.rs` already defines —
`SessionTerminalBridge`, `WorktreeSource`, `SessionTokenMinter`, `RemoteSnapshotSource` — two of which
`ConnectionServiceImpl` implements. The dependency direction is already the one extraction wants.

**Family T is the only protocol change**: `StreamLiveKitRooms` leaves
`connection.ConnectionService` for `livekit.LiveKitService`, and `packages/tddy-web`'s rooms panel
migrates in this PR.

## Boundaries

This PR explicitly does **not**:

- Move the host-key path. `host_keypair.rs`, `host_private_key.rs`, `ssh_agent.rs` and
  `ssh_agent_add.rs` went to **node 1** with the host service, because `AddHostKey` and
  `ListHostKeyCandidates` are host-service methods and because that move is what cut the
  `host_tooling ⇄ ssh_agent` cycle. This crate is therefore smaller than "auth" suggests.
- Move `daemon_settings.rs` or `daemon_config_service.rs`. They serve the daemon's own configuration;
  that is wiring and stays.
- Move `split_session.rs`. `livekit_peer_discovery.rs:529` reads one `&str` constant from it
  (`SPLIT_AGENT_IDENTITY_PREFIX`); **node 1 cut that edge**, and the session subsystem itself belongs
  to nodes 6–8.
- Take any session family. B, I–N and P–S are nodes 6–8; C, D, O and Q stay in the daemon.
- Implement `move_module_to_crate` or the kernel's resolver aliases — both node 1's, both delivered.
  **The cycle cuts are no longer node 1's**, and this line said they were. Node 1 re-derived the
  module graph at `ac002643`, found 15 mutual pairs rather than the discovery table's 9, cut six, and
  assigned the four that genuinely span destination crates to *"the nodes that move those modules"*
  ([its wrapped changeset](../changesets/2026-09-09-unbundle-host-worktree-services.md) §
  *The cycle audit*). Two of those four are this node's, and a third is one a pair-based audit could
  not see. They are listed under `## Scope`.
- Force every file under 500 lines. `session_room.rs` (2,815), `livekit_peer_discovery.rs` (2,215),
  `auth.rs` (1,058), `common_room_supervisor.rs` (768) and `livekit_rooms_stream.rs` (695) are over
  budget and are split where the seams are cohesive; whatever stays over is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate` | every move here is a plan the operation executes | add, extend or fix the operation |
| `n1` host-worktree-services | `tddy-daemon-kernel` exporting `SessionUserResolver` and `SessionsBaseResolver` | `auth.rs:25` imports the first, and it is the **entire** dependency `auth.rs` has on `connection_service` | define, re-export or re-implement either alias |
| `n1` host-worktree-services | **one** cycle cut, `config.rs:85 → session_room` — delivered by moving `config.rs` into the kernel whole and inlining `DEFAULT_SESSION_ROOM_GIT_TIMEOUT` | the wiring layer no longer reaches this crate | re-cut it, or move `config.rs` again |
| `n1` host-worktree-services | the shared `packages/tddy-service/proto/types.proto` | `livekit.proto` imports it for `SessionEntry` and the room messages, rather than duplicating them | change `types.proto`'s shape; a message it lacks is requested upward |
| `n1` host-worktree-services | the four host-key modules, already in `tddy-host-service` | nothing — this PR must simply **not** move them again | move `host_keypair`, `host_private_key`, `ssh_agent` or `ssh_agent_add` |
| **`n2` model-telegram-screen** | **`tddy-telegram`, whose modules reach `session_room`** | this PR **repoints `tddy-telegram`'s dependency** from `tddy-daemon` to `tddy-daemon-livekit`. This is a real edge, not just a branch order: `tddy-telegram` must exist before its dependency can be repointed | change anything else in `tddy-telegram`; the repoint is a one-line manifest and import change |
| `n3` sandbox-spawn-services | nothing this PR consumes | — | — |

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-daemon-auth/src/lib.rs` declaring `build_auth_entries`, `AuthBuildResult`,
  `session_token_authenticator`, `build_token_service_entry` and `LiveKitTokenServiceImpl` with real
  signatures, bodies annotated `// TODO(auth-livekit): implement`.
- `packages/tddy-daemon-livekit/src/lib.rs` declaring `SessionRoomRegistry`, `SupervisedCommonRoom`,
  `CommonRoomPeerRegistry`, `daemon_rpc_identity` and the four trait ports.
- `packages/tddy-service/proto/livekit.proto` declaring `livekit.LiveKitService.StreamLiveKitRooms`,
  importing `types.proto`.
- Both `Cargo.toml`s and their workspace `members` entries.
- The failing acceptance tests, including one asserting neither crate has `tddy-daemon` on its
  dependency path.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 3 of 3
**Greenable independently:** **not until node 2 is green.** Every other prerequisite is node 1's
published surface, but this node repoints `tddy-telegram`'s dependency on `session_room`, and
`tddy-telegram` does not exist until node 2 implements it. That is a real behavioural dependency, not
a branch-order artefact.
**Concurrent with:** nodes 6, 7 and 8 — disjoint subsystems, disjoint proto families, disjoint tests.
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8

So the true waves are: **w1** `n1`; **w2** `n2`, `n3`, `n5`; **w3** `n4`, `n6`, `n7`, `n8`.

⚠ **Recurring conflict**: `packages/tddy-daemon/src/runtime.rs`, edited by every node. Resolve by
keeping every node's removals.

## Affected Packages

- **tddy-daemon-auth** *(new)* — 7 modules; `tddy-github`'s signer stays a dependency
- **tddy-daemon-livekit** *(new)* — 5 modules; `livekit` and `tddy-livekit` move here
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 12 modules and 8,787 prod LoC leave
  - [codex-oauth-relay.md](../../packages/tddy-daemon/docs/codex-oauth-relay.md), [oauth-loopback-tunnel.md](../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md) → `tddy-daemon-auth/docs/`
  - [session-room.md](../../packages/tddy-daemon/docs/session-room.md) → `tddy-daemon-livekit/docs/`
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — loses the `StreamLiveKitRooms` entry
- **tddy-telegram**: its `session_room` dependency repoints
- **tddy-integration-tests**: its `tddy_daemon::codex_oauth_relay` dependency repoints — the only thing
  it reaches the daemon for
- **tddy-service**: `livekit.proto` appears; `connection.proto` loses 1 rpc
- **tddy-web**: `src/rpc/useLiveKitRooms.ts`, the rooms panel, and `cypress/support/rpc/liveKitRoomsBackend.ts`
- **tddy-desktop**: no change expected. **Outside the CI gate**; verified locally and stated

## Related Feature Documentation

- [PRD-2026-09-09-auth-livekit.md](../../ft/daemon/1-WIP/PRD-2026-09-09-auth-livekit.md)

## Summary

The identity boundary becomes `tddy-daemon-auth` and everything LiveKit becomes
`tddy-daemon-livekit`. Auth's four proto services move without a protocol change; LiveKit takes one
method, `StreamLiveKitRooms`, into a new `livekit.LiveKitService`, and the web rooms panel migrates.

## Background

See the PRD. The one fact worth repeating here because it constrains both crates:
**`config.livekit.api_secret` is the single secret that signs both LiveKit room JWTs and session
tokens**, through `tddy_github::SessionTokenSigner`. Splitting auth from LiveKit into two crates does
not split that secret, and neither crate may start deriving its own.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`

`github_token_store.rs` moves to a new crate in this PR, and the entry records that the daemon's
secret stores truncate a file in place rather than writing atomically — a crash mid-write leaves a
truncated secret at rest. Every route around it while *moving* the store is wrong: relocating the
code verbatim carries a known data-loss path into a crate whose whole purpose is to be the identity
boundary, and hand-rolling an atomic write beside `tddy_core::atomic_file::write_atomic` deepens the
duplication the entry is about. The fix is to route the store through the existing helper as part of
the move. Earns a `## Scope` line.

### ⚠ DURING — `2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md` and `2026-09-06-the-livekit-room-creation-call-has-no-timeout.md`

Both are in this node's path — `session_room.rs` and `common_room_supervisor.rs` are exactly where
they bite. Neither is fixed here: adding a deadline changes live behaviour under load and belongs in
its own PR with its own test. The move must not deepen them, so no new un-deadlined LiveKit call is
introduced and the existing ones move verbatim. Re-read both at wrap, when the calls are all in one
crate and a single deadline policy is finally expressible.

### ⚠ DURING — `2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md` and `2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md`

Open defects inside the subsystem being moved. They survive the move unchanged; recorded so a reviewer
seeing them in `tddy-daemon-livekit` knows they are inherited.

### ℹ ANSWERED — `2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`

Records that *"Peer discovery does not follow a runtime common-room reconnect"*, with the `TODO` at
`DaemonCommonRoomConnector::connect`. Discovery answers the structural half of why: the connector sits
behind `common_room_supervisor → daemon_config_service → livekit_peer_discovery`, a cycle through the
wiring layer. Node 1 cuts it, and after this node the connector and the registry are in one crate —
so the reconnect path becomes expressible without reaching through `config`. Still not implemented
here; the entry should be updated with the new location at wrap.

## Scope

- [x] **`tddy-daemon-auth`**: crate, 7 modules, its four services, its suites ✅ — the 7th is
      `github_pr_credentials.rs`, which the count implied but never named
- [x] **⛔ Atomic secret writes** ✅ — `github_token_store` routed through
      `tddy_core::atomic_file::write_atomic_with_mode`. The mode-aware variant, not `write_atomic`:
      the plain one carries permissions over from an *existing* target, so a **first** write would
      create the swap file at the process umask and publish a world-readable credential
- [ ] **`tddy-daemon-livekit`**: crate, 5 modules, 18 test suites
- [ ] **Three cycle cuts, inherited from node 1's audit** — each spans a crate boundary only because
      this node moves one end, so each is this node's:
  - [ ] `livekit_peer_discovery.rs:493 → split_session::SPLIT_AGENT_IDENTITY_PREFIX` — named by node 1
  - [ ] `host_registry ⇄ livekit_peer_discovery` — named by node 1; `host_registry` left with node 1
  - [ ] `common_room_supervisor.rs:29 → daemon_config_service.rs:213 → livekit_peer_discovery` — a
        three-hop loop node 1's pair-based audit could not see. Both ends move here;
        `daemon_config_service` stays by `## Boundaries`
- [ ] **Proto**: `livekit.proto` with family T; `connection.proto` loses `StreamLiveKitRooms`
- [x] **Repoint**: `tddy-integration-tests` → `tddy-daemon-auth` ✅ — its `tddy-daemon` dependency is
      gone entirely, not merely joined
- [ ] ⏸ **Deferred — `tddy-telegram` → `tddy-daemon-livekit`** (M6). Node 2 (PR #471) is still at its
      draft contract: `packages/tddy-telegram/src/lib.rs` is a 133-line stub whose `Cargo.toml` has no
      `tddy-daemon` dependency, so there is nothing to repoint. Picked up by a `/pr-stack-rebase` here
      once #471 greens. **This node cannot merge with M6 open.**
- [ ] **Web**: the rooms panel migrated to `livekit.LiveKitService`; the Cypress fake moved
- [ ] **Docker-dependent suites**: skip behaviour unchanged when `/var/run/docker.sock` is absent
- [ ] **File budget**: record which over-500-line files landed under budget and which did not, with why
- [ ] **Baseline**: `./test` per touched package back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | Files | prod LoC | inline tests | Reaches `connection_service` for |
|---|---:|---:|---:|---|
| auth/secrets (post-node-1) | 7 | 2,145 | ~1,000 | `SessionUserResolver` (`auth.rs:25`) — nothing else |
| LiveKit | 5 | 6,642 | ~1,400 | nothing in code; doc comments only |

Three cycles blocked this node before node 1: `config.rs:85 → session_room::DEFAULT_GIT_TIMEOUT`,
`common_room_supervisor → daemon_config_service → livekit_peer_discovery`, and
`livekit_peer_discovery.rs:529 → split_session::SPLIT_AGENT_IDENTITY_PREFIX`.
`livekit_peer_discovery ⇄ multi_host` is resolved by putting both in one crate.
`livekit` and `tddy-livekit` are attributable to this subsystem; the crypto crates left with node 1.

### State B

`tddy-daemon` loses 12 modules and 8,787 prod LoC. `tddy-daemon-auth` exposes `build_auth_entries`;
`tddy-daemon-livekit` exposes its registry constructors and four trait ports and serves
`livekit.LiveKitService`. `tddy-telegram` and `tddy-integration-tests` depend on the new crates.
`connection.ConnectionService` is down to **72 methods**.

### Delta

#### tddy-daemon
- **Architecture**: 12 modules leave; `lib.rs` loses 12 entries; the common-room supervisor task
  leaves `RuntimeTasks`
- **Implementation**: `runtime.rs`'s auth entry group, token service and LiveKit registrations move
  behind the new crates' constructors

#### tddy-daemon-auth
- **API**: `build_auth_entries`, `AuthBuildResult`, `session_token_authenticator`,
  `build_token_service_entry`, `LiveKitTokenServiceImpl` — shapes unchanged
- **Implementation**: the secret store writes atomically (⛔ prerequisite)

#### tddy-daemon-livekit
- **API**: `SessionRoomRegistry`, `SupervisedCommonRoom`, `CommonRoomPeerRegistry`,
  `daemon_rpc_identity`, and the four trait ports — shapes unchanged
- **Proto**: serves `livekit.LiveKitService`

#### tddy-service
- **Proto**: `livekit.proto` added, importing `types.proto`; `connection.proto` loses 1 rpc and the
  messages that move with it
- **Build**: one prost + one tonic pass; added to the descriptor set

#### tddy-web
- **Implementation**: `useLiveKitRooms.ts` targets `livekit.LiveKitService`; the Cypress fake moves

## Implementation Milestones

- [x] M1 — `tddy-daemon-auth` extracted; its four services answer; no `tddy-daemon` on its path ✅
- [x] M2 — the secret store writes atomically; a crash mid-write leaves the old value intact ✅
- [x] M3 — `tddy-integration-tests` repointed ✅
- [ ] M4 — `livekit.proto` generates; `types.proto` imported rather than duplicated
- [ ] M5 — `tddy-daemon-livekit` extracted; 18 suites pass; no reach into `split_session` or `daemon_config_service`
- [ ] M6 — ⏸ **deferred**: `tddy-telegram` repointed. Blocked on node 2 greening; see `## Scope`
- [ ] M7 — web rooms panel migrated; Cypress component suites green
- [ ] M8 — Docker-skip behaviour verified unchanged; baselines restored; file budget recorded

## Testing Plan

**Primary test level: integration, per package.** 7,092 LoC of dedicated suites move with the code.

Beyond the moved suites, four proofs:

- **`restructure verify --against <pre-move ref>`** from the repo root.
- **The moved-line diff**, alongside the visibility table, never instead of it.
- **A dependency-path assertion per crate** — neither `tddy-daemon-auth` nor `tddy-daemon-livekit` may
  have `tddy-daemon` on its dependency path. This is the check that proves the extraction is real
  rather than a re-export.
- **A Docker-absence audit.** Several LiveKit suites abort in harness setup without
  `/var/run/docker.sock`, and `docs/dev/todo/2026-09-05-…` records that this leaves the real LiveKit
  join path unexercised. Their skip behaviour must be identical after the move — otherwise a suite
  that was silently skipped starts failing, or one that ran stops.

The atomic-write prerequisite gets a real test: write a secret, simulate a failure between truncate
and write, and assert the previous value survives.

`tddy-web` keeps `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

### tddy-daemon-auth
- [ ] **Integration**: all 5 `auth.AuthService` methods answer from the new crate (`auth_service_acceptance.rs`)
- [ ] **Integration**: `MintLiveKitToken` and `token.TokenService` mint against `config.livekit.api_secret` (`token_service_acceptance.rs`)
- [ ] **Integration**: a token signed by this crate authenticates a call to a service in another crate (`cross_crate_session_token_acceptance.rs`)
- [ ] **Unit**: a failure between truncate and write leaves the previous secret intact (`github_token_store.rs`)
- [ ] **Unit**: `tddy-daemon` is absent from this crate's dependency path (`dependency_boundary_unit.rs`)

### tddy-daemon-livekit
- [ ] **Integration**: `livekit.LiveKitService.StreamLiveKitRooms` streams and terminates cleanly (`stream_livekit_rooms_rpc.rs`)
- [ ] **Integration**: a session room opens, publishes and is re-joined across hosts (`session_room_acceptance.rs`, `session_room_cross_host_acceptance.rs`)
- [ ] **Integration**: common-room peer discovery lists another daemon (`livekit_peer_daemons_acceptance.rs`)
- [ ] **Unit**: `tddy-daemon` is absent from this crate's dependency path (`dependency_boundary_unit.rs`)

### tddy-telegram
- [ ] **Integration**: a telegram-started session still reaches its room, through `tddy-daemon-livekit` (`telegram_start_claude_acceptance.rs`)

### tddy-web
- [ ] **Cypress component**: the LiveKit rooms panel loads through `livekit.LiveKitService` (`LiveKitRoomsPanel.cy.tsx`)

### tddy-daemon
- [ ] **Integration**: `connection.ConnectionService` no longer declares `StreamLiveKitRooms`, and
      `livekit.LiveKitService` is registered (`service_registration_acceptance.rs`)

## Decisions & Trade-offs

- **Auth and LiveKit share a node.** They are unrelated subsystems, and under a 25-node plan they were
  two PRs. They are one here because of the consolidation to 8 — and the pairing is the least
  arbitrary available, because they share the one secret that signs both room JWTs and session tokens.
  A reviewer of this node sees that fact once, where a reviewer of either half alone would not see it.
- **The auth crate is smaller than "auth" suggests.** The host-key path — `host_keypair`,
  `host_private_key`, `ssh_agent`, `ssh_agent_add`, 1,994 prod LoC — went to node 1 with the host
  service, because `AddHostKey` is a host-service method and because that move cut the
  `host_tooling ⇄ ssh_agent` cycle. Splitting "secrets" from "identity" this way is a real boundary,
  not a convenience: what is left here signs and verifies, and what left with node 1 unlocks and loads.
- **`multi_host.rs` moves with LiveKit, not with hosts.** Its name suggests the host subsystem, but
  its content is LiveKit peer routing and it is mutually referential with
  `livekit_peer_discovery.rs`. Putting both in one crate resolves that cycle by containment rather
  than by cutting it.
- **The atomic-write fix is in scope, and that is a deliberate exception to "move only".** The entry is
  blocking rather than merely annoying: this crate exists to be the identity boundary, and shipping it
  with a known truncate-in-place path at its centre would be worse than a slightly larger diff. The
  fix is a call to an existing helper, not new machinery.
- **Family T moves even though it is one method.** Leaving it in `connection.ConnectionService` would
  mean the daemon retains a handler for a subsystem that lives elsewhere, which is exactly the shape
  this stack exists to remove. One method is also the cheapest possible second exercise of the
  proto-split mechanics node 1 established.

## Technical Debt & Production Readiness

- [ ] LiveKit calls still have no client-side deadline; after this node they are all in one crate, so
      a single policy becomes expressible. Recorded, not fixed
- [ ] The Docker-dependent suites still leave the real LiveKit join path unexercised
- [ ] `tddy-service` depends on `tddy-tui`, so `tddy-daemon-livekit` inherits the TUI in its build

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | |
| `cargo clippy -p tddy-daemon-auth -p tddy-daemon-livekit -p tddy-service --all-targets -- -D warnings` | ✅ exit 0 | |
| LiveKit suites with `/var/run/docker.sock` absent (skip count) | not yet measured — no suite has moved in commit 2 | |

**9 failing tests** define this node: 3 in `tddy-daemon-auth` (one of them the ⛔ atomic-write
prerequisite), 3 in `tddy-daemon-livekit`, and 3 in `tddy-service` — the two inherited from node 1
plus one asserting `StreamLiveKitRooms` has actually left `connection.ConnectionService`.

`livekit.proto` turned out to be a third self-contained cut: its closure is **12 messages with zero
overlap** with anything that stays, so like `host.proto` and `worktree.proto` it imports nothing.

The known pre-existing failure inherited from node 1's baseline is expected to stay at exactly one.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-auth-livekit.md` — the release-note file, carrying the
      `StreamLiveKitRooms` coordinate change and the atomic-write fix
- [ ] Move `codex-oauth-relay.md` and `oauth-loopback-tunnel.md` to `tddy-daemon-auth/docs/`;
      `session-room.md` to `tddy-daemon-livekit/docs/`
- [ ] `packages/tddy-daemon/docs/connection-service.md` — remove the `StreamLiveKitRooms` entry
- [ ] `docs/ft/web/livekit-rooms-panel.md` — the new coordinate
- [ ] ⚠ **Do not close** `docs/dev/todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`.
      It names three stores; this node fixed one. `vnc_vault.rs:153` and `screen_sharing_vault.rs:171`
      still carry `.truncate(true)` and belong to **node 2** (`tddy-screen-sharing`). Narrow the entry
      to the two that remain rather than closing it
- [ ] Update `2026-09-05-…-tauri-desktop-single-process-daemon.md` with the connector's new location
- [ ] Re-read the two LiveKit-deadline entries now that every call is in one crate
- [ ] Doc triage: `grep -rn -e 'auth' -e 'session_room' -e 'livekit_peer' -e 'StreamLiveKitRooms' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon docs/ft/web`
