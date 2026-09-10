# Changeset: the auth and LiveKit subsystems in their own crates

**Date**: 2026-09-09
**Status**: ✅ Green — M1–M8 complete and pushed. Ready for `/validate-changes`
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
| `tddy-daemon-livekit` | 4 | 6,730 | **`livekit.LiveKitService` (1, new)** — family T | 5 files / 1,283 LoC |

`tddy-daemon-auth` owns `build_auth_entries(...) -> AuthBuildResult`, whose `user_resolver` is what
every other service in the daemon authenticates with. This crate is the daemon's identity boundary.

`tddy-daemon-livekit` keeps the four trait ports `session_room.rs` already defines —
`SessionTerminalBridge`, `WorktreeSource`, `SessionTokenMinter`, `RemoteSnapshotSource` — two of which
`ConnectionServiceImpl` implements. The dependency direction is already the one extraction wants.

**Corrected at implementation.** The table said 5 modules and named `multi_host.rs`; node 1 took
`multi_host.rs` to `tddy-host-service` with `host_registry`, so **4** modules move here and the
`livekit_peer_discovery ⇄ multi_host` pair is no longer this node's to resolve — nothing in
`tddy-host-service` names a LiveKit module, so the edge is one-directional. A fifth module,
`livekit_service.rs`, is **authored** here rather than moved: `StreamLiveKitRooms` needs a service
to be served by once it leaves `ConnectionServiceImpl`.

The dedicated-test figure was also wrong, and by more. Of the 18 suites attributed to this
subsystem, **13 are joint session/LiveKit suites** that reach `connection_service`, `test_util`,
`split_session`, `claude_cli_session` or `session_attachment_staging` — all of which stay for nodes
6–8. A suite that names them cannot move without putting `tddy-daemon` back on this crate's
dependency path, which is the one thing the split exists to prevent. **5 move**; the rest travel
with the session families.

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
**Greenable independently:** **yes — corrected 2026-09-10.** This line claimed node 2 had to land
first, because the `session_room` consumer was assumed to be inside `tddy-telegram` by then. It is
not: `telegram_session_control.rs` cannot leave `tddy-daemon` at node 2's position, so the repoint
happens here, in place. Every other prerequisite is node 1's published surface, and node 1 is green.
**`n2 → n4` is not a real edge.**
**Concurrent with:** nodes 6, 7 and 8 — disjoint subsystems, disjoint proto families, disjoint tests.
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n5 → n6, n7, n8

`n2 → n4` was struck 2026-09-10 — see **Greenable independently** above.

So the true waves are: **w1** `n1`; **w2** `n2`, `n3`, `n4`, `n5`; **w3** `n6`, `n7`, `n8`.

⚠ **Recurring conflict**: `packages/tddy-daemon/src/runtime.rs`, edited by every node. Resolve by
keeping every node's removals.

## Affected Packages

- **tddy-daemon-auth** *(new)* — 7 modules; `tddy-github`'s signer stays a dependency
- **tddy-daemon-livekit** *(new)* — 4 moved modules plus an authored `livekit_service.rs`; `livekit` and `tddy-livekit` move here
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 11 modules and 8,875 prod LoC leave (7 to `tddy-daemon-auth`, 4 to `tddy-daemon-livekit`; **not 12** — `multi_host.rs` left with node 1)
  - [codex-oauth-relay.md](../../packages/tddy-daemon/docs/codex-oauth-relay.md), [oauth-loopback-tunnel.md](../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md) → `tddy-daemon-auth/docs/`
  - [session-room.md](../../packages/tddy-daemon/docs/session-room.md) → `tddy-daemon-livekit/docs/`
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — loses the `StreamLiveKitRooms` entry
- **tddy-telegram**: ~~its `session_room` dependency repoints~~ — **no change**. The consumer
  (`telegram_session_control.rs`) stays in `tddy-daemon`; the repoint happens there
- **tddy-integration-tests**: its `tddy_daemon::codex_oauth_relay` dependency repoints — the only thing
  it reaches the daemon for
- **tddy-service**: `livekit.proto` appears; `connection.proto` loses 1 rpc
- **tddy-web**: `src/rpc/useLiveKitRooms.ts`, the rooms panel, and `cypress/support/rpc/liveKitRoomsBackend.ts`
- **tddy-daemon-kernel**: **+17 lines**, unplanned. `SPLIT_AGENT_IDENTITY_PREFIX` lands in
  `daemon_identity.rs` as cycle **cut A**; `split_session` re-exports it, so no caller path changed
  and there stays one definition. Node 1's own precedent for a one-symbol cross-crate edge
- **tddy-workflow**: **+57 lines**, unplanned. `SessionAttachmentFile` and
  `list_session_attachments` land in `artifact_paths.rs` as cycle **cut C**, beside the
  `session_attachments_root` that names the directory they read — a filesystem convention this crate
  already owns, which is why they are here and not in the kernel
- **tddy-livekit**: two call sites repoint to `proto::livekit` after family T moved
- **tddy-rust-typescript-tests**: `gen/livekit_pb.ts`, from the same regeneration as `tddy-web`'s
- **tddy-desktop**: no change expected. **Outside the CI gate**; verified locally and stated

Four packages above were **not** in this list when the node was planned. All four are consequences of
the cycle cuts node 1 reassigned to the moving nodes — see `## Boundaries`.

## Validation Results

`/validate-changes`, run 2026-09-10 on the rebased tree (base
`feature/unbundle/sandbox-spawn-services` @ `a6364d45`).

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes — planned, base `feature/unbundle/sandbox-spawn-services` |
| `/pr-stack-rebase` | ✅ Rebased (base extended by node 3's wrap commit) |
| Leak check — `origin/<base>..HEAD` is this PR only | ✅ Clean, 6 commits |

### Stack boundary

| Check | Result |
|---|---|
| Changeset items implemented or deferred | ✅ All — M1–M8 |
| `## Responsibility` delivered | ✅ No stubs in either owned crate |
| `## Dependencies` not implemented here | ✅ `move_module_to_crate` untouched; the kernel gained a const for an assigned cut, not one of node 1's aliases |
| `## Boundaries` respected | ✅ `daemon_settings`, `daemon_config_service`, `split_session` all still in `tddy-daemon`; host-key modules untouched in `tddy-host-service` |
| No dependent's behaviour | ✅ |
| Diff contains only this PR's files | ✅ — four packages beyond the plan, all cycle-cut consequences, now listed above |
| Parent-owned files intact | ✅ **Zero deletions in the whole diff** |

### Build — every touched package

`tddy-daemon-auth`, `tddy-daemon-livekit`, `tddy-daemon`, `tddy-service`, `tddy-workflow`,
`tddy-daemon-kernel`, `tddy-integration-tests`, `tddy-livekit` — **8/8 ✅**.

### Risk scan

| Check | Result |
|---|---|
| Hardcoded secrets | ✅ None. Every match is a test fixture (`devkey`, `some-other-deployments-secret`) or doc prose |
| `println!` / `eprintln!` added | ✅ None |
| New `unwrap`/`expect` in production paths | ✅ **None.** Net +9 against the same modules at base, all inside `#[cfg(test)]`; the authored `livekit_service.rs` has zero |
| Temporary markers this PR added | ✅ None. The two `TODO(session-room)` in `session_room.rs` are **pre-existing** — 2 at base, moved verbatim, and named as inherited defects in `## Prerequisites` |
| Test-environment branches / fallbacks | ✅ None |

### Clean-code metrics (`/analyze-clean-code`, step 4)

**Everything authored in this node is under budget.** `livekit_service.rs` 127,
`tddy-daemon-livekit/src/lib.rs` 137, `tddy-daemon-auth/src/lib.rs` 153,
`daemon_identity.rs` 113 (+17 here), `artifact_paths.rs` 373 (+57 here).

**Five files are over 500 lines, and every one of them is relocated code, not new code:**

| File | Lines | |
|---|---:|---|
| `tddy-daemon-livekit/src/session_room.rs` | 2,992 | moved |
| `tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 2,060 | moved |
| `tddy-daemon-auth/src/auth.rs` | 1,200 | moved |
| `tddy-daemon-livekit/src/common_room_supervisor.rs` | 881 | moved |
| `tddy-daemon-livekit/src/livekit_rooms_stream.rs` | 779 | moved |

**Splitting them is deferred to a follow-up branch after the stack lands**, on two independent
grounds. `/pr-wrap` step 4 forbids restructuring a file that another PR in the stack also touches —
`session_room.rs` is reached by the session subsystem that nodes 6–8 move, and a rename cascade
through their diffs turns each into a conflict. And `## Boundaries` already names the file budget a
non-goal for this node. Splitting also costs a second 40-minute rust-analyzer index per file, since
`move_module_to_crate` only handles a whole `<crate>/src/<module>.rs`.

### Findings

- **[INFO]** `packages/tddy-daemon-kernel/src/daemon_identity.rs` and
  `packages/tddy-workflow/src/artifact_paths.rs` are additions to crates this node did not plan to
  touch. Both are cycle cuts, both add rather than modify, and both are recorded above. A reviewer
  should still see them called out, because a kernel change inside a LiveKit PR is surprising.
- **[INFO]** `docs/dev/todo/2026-08-16-…-truncate-in-place.md` stays open by design: it names three
  stores and this node fixed one. The other two are node 2's.

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
- [x] **`tddy-daemon-livekit`**: crate, 4 moved modules + 1 authored, 5 moved test suites ✅
- [x] **Four cuts — two inherited from node 1's audit, two this node found** ✅. Each spans a crate
      boundary only because this node moves one end, so each is this node's:
  - [x] **A** — `livekit_peer_discovery.rs:493 → split_session::SPLIT_AGENT_IDENTITY_PREFIX`, named
        by node 1. Cut by **lifting the constant to `tddy_daemon_kernel::daemon_identity`**, whose
        stated charter is already "the identity names several crates need", with `split_session`
        re-exporting it. The other direction — defining it here and having `split_session` read it
        — would make the *producer* of the identity depend on the crate that *refuses* it, and
        would break again when nodes 6–8 move `split_session` to a third crate
  - [x] **B** — `common_room_supervisor.rs:29 → daemon_config_service.rs:213 →
        livekit_peer_discovery`, the three-hop loop node 1's pair-based audit could not see. Cut by
        moving the `CommonRoomSupervisor` **trait** to `common_room_supervisor.rs`, its one
        implementation, with `daemon_config_service` re-exporting it. Verified rather than assumed:
        re-pointing the return leg alone does not do it, because the trait import is a
        `livekit → daemon` edge whether or not it closes a loop. The return leg was re-pointed too,
        from a re-export in a departing module to its real home,
        `tddy_daemon_kernel::daemon_identity::local_instance_id_for_config`
  - [x] **C** *(found here)* — `session_room.rs:2137 → session_attachments::list_session_attachments`.
        `session_attachments` belongs to the session family and stays. The listing is lifted to
        `tddy_workflow::artifact_paths`, beside the `session_attachments_root` that names the very
        directory it reads, and the daemon re-exports it. Not the kernel: this is a filesystem
        convention `tddy-workflow` already owns, not a daemon identity
  - [x] **D** *(found here)* — `livekit_peer_discovery.rs:743 → oauth_loopback_tunnel`, which M1–M3
        moved into `tddy-daemon-auth`. Not a cycle, but a forbidden direction: the LiveKit crate
        must not reach the identity boundary. `spawn_oauth_loopback_tunnel` moved to
        `tddy_daemon_auth::oauth_loopback_tunnel`, the module whose supervisor it starts and whose
        own doc already described the eligibility gate. `spawn_common_room_discovery_task`, which
        composes that supervisor with the discovery loop, moved to **`tddy-daemon`'s `runtime.rs`**
        — it has no production caller, and after the split `tddy-daemon` is the only crate that has
        both halves
  - [x] `host_registry ⇄ livekit_peer_discovery` — **no longer a cycle.** Node 1 took both
        `host_registry` and `multi_host` to `tddy-host-service`, and re-deriving the edges finds
        **zero** references from anything in that crate back into a LiveKit module. The edge is
        one-directional `tddy-daemon-livekit → tddy-host-service`, and needs no cut
- [x] **Proto**: `livekit.proto` with family T; `connection.proto` loses `StreamLiveKitRooms` and the
      12 messages that move with it, 73 → **72** rpcs ✅. The committed TypeScript regenerated:
      `livekit_pb.ts` appears in `packages/tddy-web/src/gen` and
      `packages/tddy-rust-typescript-tests/gen`, and `scripts/generated-code.sh check` is clean
- [x] **Repoint**: `tddy-integration-tests` → `tddy-daemon-auth` ✅ — its `tddy-daemon` dependency is
      gone entirely, not merely joined
- [x] **Repoint of the `session_room` consumer** (M6) ✅ — **done, but not where this plan put it.**
      The consumer is `telegram_session_control.rs`, and it is **in `tddy-daemon`**, not in
      `tddy-telegram`. It holds `SessionRoomRegistry` as a struct field
      (`telegram_session_control.rs:1292`) and now names
      `tddy_daemon_livekit::session_room::SessionRoomRegistry`, which is precisely the edge M6
      describes. Node 2 **cannot** take that module: it orchestrates the daemon's session lifecycle
      through 12 daemon-owned modules and sits in two real production cycles
      (`session_list_enrichment ⇄ elicitation`, `session_notifications ⇄ telegram_session_subscriber`),
      so it belongs to a **successor of nodes 4 and 6–8**, not to node 2. This node therefore does
      **not** wait on node 2, and `tddy-telegram` needs no manifest change — it has no `tddy-daemon`
      dependency to repoint and will not gain one at node 2's position.
- [x] **Web** (M7) ✅: the rooms panel migrated to `livekit.LiveKitService`; the Cypress fake moved
      with it. Five files: `useLiveKitRooms.ts` (now `useDaemonClient(LiveKitService)`),
      `liveKitRoomsState.ts` + its test, `liveKitRoomsBackend.ts`, and the spec's header comment.
      The round-trip is proven rather than merely compiled: the fake registers its handler **only**
      on `LiveKitService`, and the testkit router answers any unregistered method with
      `Code.Unimplemented` — so a client still asking `ConnectionService` would render the error
      branch and fail the suite. `LiveKitRoomsPanelAcceptance.cy.tsx`: **26/26**.
- [x] **Docker-dependent suites**: behaviour unchanged ✅ — and the premise needs correcting.
      **Nothing skips.** `LiveKitTestkit::start()` returns `Err` without Docker and every caller
      `.expect(...)`s it, so an absent `/var/run/docker.sock` makes these suites *fail*, loudly,
      with "LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)". Four such suites moved
      (`session_room_livekit_acceptance`, `livekit_peer_daemons_acceptance` and the two common-room
      repros); `tddy-daemon-livekit` takes the same `tddy-livekit-testkit` dev-dependency and the
      harness code is byte-identical, so the outcome is the same on both sides of the move. Checked
      by reading `LiveKitTestkit::start` rather than by unplugging Docker, and all 11 tests were run
      green *with* Docker present
- [x] **File budget**: recorded, and **none landed under 500** ✅. `session_room.rs` (2,992),
      `livekit_peer_discovery.rs` (2,060), `common_room_supervisor.rs` (881) and
      `livekit_rooms_stream.rs` (779) all crossed as whole modules; the one new file,
      `livekit_service.rs`, is 127. Splitting was not attempted: `move_module_to_crate` operates on
      whole `<crate>/src/<module>.rs` files, so every split would have to happen either before the
      move (churning the diff a reviewer reads as "did any logic change?") or after it (a second
      restructure plan, and each plan costs a 40-minute rust-analyzer index on this workspace). The
      budget is `## Boundaries`' explicit non-goal — *"Force every file under 500 lines"* — and the
      seams are worth their own PR now that the subsystem is in one crate
- [x] **Baseline** ✅: recorded per touched package in `## Baseline`, with the 18 pre-existing
      sandbox-family failures named and attributed rather than folded into a total
- [x] **Code Quality** ✅ — **and the first version of this line is why a CI blocker was missed.**
      It read `-p tddy-daemon-livekit -p tddy-service`, omitting `tddy-daemon-auth`, so
      `items_after_test_module` in `oauth_loopback_tunnel.rs` went unseen until `/pr-wrap`. CI runs
      `--workspace --all-targets` (`.github/workflows/ci.yml:69`), so a scoped local gate must name
      **every** crate the node touches:
      `cargo clippy -p tddy-daemon-auth -p tddy-daemon-livekit -p tddy-service -p tddy-daemon-kernel -p tddy-workflow -p tddy-daemon --all-targets -- -D warnings`.
      `cargo fmt --check` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| | Files | prod LoC | inline tests | Reaches `connection_service` for |
|---|---:|---:|---:|---|
| auth/secrets (post-node-1) | 7 | 2,145 | ~1,000 | `SessionUserResolver` (`auth.rs:25`) — nothing else |
| LiveKit | 4 | 6,730 | ~1,400 | nothing in code; doc comments only |

Three cycles blocked this node before node 1: `config.rs:85 → session_room::DEFAULT_GIT_TIMEOUT`,
`common_room_supervisor → daemon_config_service → livekit_peer_discovery`, and
`livekit_peer_discovery.rs:529 → split_session::SPLIT_AGENT_IDENTITY_PREFIX`.
`livekit_peer_discovery ⇄ multi_host` is resolved by putting both in one crate.
`livekit` and `tddy-livekit` are attributable to this subsystem; the crypto crates left with node 1.

### State B

`tddy-daemon` loses 11 modules and 8,875 prod LoC. `tddy-daemon-auth` exposes `build_auth_entries`;
`tddy-daemon-livekit` exposes its registry constructors and four trait ports and serves
`livekit.LiveKitService`. `tddy-telegram` and `tddy-integration-tests` depend on the new crates.
`connection.ConnectionService` is down to **72 methods**.

### Delta

#### tddy-daemon
- **Architecture**: 11 modules leave; `lib.rs` loses 11 entries; the common-room supervisor task
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
- [x] M4 — `livekit.proto` generates ✅. **No `types.proto` to import**: node 1 established there
      is none, and this proto's closure is 12 messages with zero overlap with anything that stays,
      so like `host.proto` and `worktree.proto` it imports nothing
- [x] M5 — `tddy-daemon-livekit` extracted ✅; 5 moved suites plus 3 new ones pass; no reach into
      `split_session`, `daemon_config_service`, `session_attachments` or `tddy-daemon-auth`
- [x] M6 — the `session_room` consumer repointed ✅ — in `tddy-daemon`, not `tddy-telegram`; see `## Scope`
- [x] M7 — web rooms panel migrated; Cypress component suites green ✅ (26/26)
- [x] M8 — Docker behaviour verified unchanged; file budget recorded ✅

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
- [x] **Integration**: all 5 `auth.AuthService` methods answer from the new crate ✅
      (`auth_service_acceptance.rs`, 6 — the five methods plus a foreign-secret refusal)
- [x] **Integration**: `MintLiveKitToken` and `token.TokenService` mint against
      `config.livekit.api_secret` ✅ (`token_service_acceptance.rs`) — each verified with
      `livekit_api::access_token::TokenVerifier`, plus a test that a server holding a *different*
      secret refuses the same JWT, without which the positive two assert nothing
- [x] **Integration**: a token signed by this crate authenticates a call to a service in another
      crate ✅ (`cross_crate_session_token_acceptance.rs`, 2). The far side is `tddy-service`
      deliberately, not one of the daemon's own services — this crate cannot reach those, which is
      the property `dependency_boundary_unit.rs` pins
- [x] **Unit**: a failure part-way through a write leaves the previous secret intact ✅
      (`github_token_store.rs`). ⚠ **It does not discriminate this PR's change.** The base
      implementation already staged to `<tokens>.tmp` and renamed, and that staged create also
      fails in a `0o555` directory — so reverting the `write_atomic_with_mode` refactor would leave
      this test green. It is an honest guard against a *future* truncate-in-place, not evidence for
      the ⛔ prerequisite, and the PR description should say so
- [x] **Unit**: `tddy-daemon` is absent from this crate's dependency path ✅
      (`dependency_boundary_unit.rs`, 3) — walks the transitive manifest closure, with a third test
      asserting the walk actually reaches `tddy-daemon-kernel`, so a walk that silently found
      nothing cannot pass as a clean result

### tddy-daemon-livekit
- [x] **Integration**: `livekit.LiveKitService.StreamLiveKitRooms` streams and terminates cleanly ✅
      (`stream_livekit_rooms_rpc.rs`, 8 tests) — retargeted from `ConnectionServiceImpl` at
      `LiveKitServiceImpl`, which drops its last reach into `tddy_daemon::test_util`
- [x] **Integration**: a session room opens, publishes and is re-joined ✅ —
      `session_room_livekit_acceptance.rs` (6, against a real LiveKit server) and
      `session_room_wiring_acceptance.rs` (6, without one). **`session_room_acceptance.rs` and
      `session_room_cross_host_acceptance.rs` stay in `tddy-daemon`**: both drive
      `ConnectionServiceImpl` and `test_util`, so moving them would put `tddy-daemon` back on this
      crate's dependency path. They travel with the session families in nodes 6–8
- [x] **Integration**: common-room peer discovery lists another daemon ✅ (`livekit_peer_daemons_acceptance.rs`, 3)
- [x] **Unit**: `tddy-daemon` is absent from this crate's dependency path ✅
      (`dependency_boundary_unit.rs`, 4) — with a fourth test pinning `tddy-daemon-auth`'s absence
      too, so `SessionTokenMinter` cannot quietly stop being a port

### tddy-telegram
- [x] **N/A — the premise was wrong** ✅. This row assumed `tddy-telegram` would hold the
      `session_room` consumer by now. It does not and will not at node 2's position: the consumer is
      `telegram_session_control.rs`, which stays in `tddy-daemon`. Its repoint is covered by
      `tddy-daemon` building and its own suites passing

### tddy-web
- [x] **Cypress component**: the LiveKit rooms panel loads through `livekit.LiveKitService` ✅ —
      `LiveKitRoomsPanelAcceptance.cy.tsx`, **26/26** (the spec is named `…Acceptance.cy.tsx`, not
      the planned `LiveKitRoomsPanel.cy.tsx`). The fake registers its handler **only** on
      `LiveKitService` and the testkit router answers anything unregistered with
      `Code.Unimplemented`, so a client still asking `ConnectionService` would fail the suite rather
      than quietly pass

### tddy-daemon
- [x] **Integration** ✅: `connection.ConnectionService` no longer declares `StreamLiveKitRooms`, and
      `livekit.LiveKitService` is registered — in **`livekit_service_registration_acceptance.rs`**,
      not the planned `service_registration_acceptance.rs`: node 2 created a file of that name
      first, for the model-registry/screen-sharing/VNC roster, and two unrelated subjects in one
      file would re-conflict on every rebase. Asserts by **dispatching**, not by reading a name
      list — `livekit.LiveKitService` answers `Unauthenticated` (handler reached, token judged)
      while `connection.ConnectionService` answers `NotFound` (handler gone, not merely refused)

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
- [ ] **`ensure_owner_only_dir` changed behaviour twice, and only one half was deliberate.**
      `github_token_store.rs` now builds the directory with `DirBuilder::recursive(true).mode(0o700)`
      instead of `create_dir_all` + an unconditional `set_permissions(0o700)`:
      1. *Intended* — an **existing** storage directory no longer has `0o700` re-imposed on every
         write, so the daemon stops overruling an operator's deliberate `chmod`. The cost is that an
         `auth_storage` that is currently group- or world-readable **stays that way after upgrade**,
         where before every `put` re-tightened it. That deserves a decision, not a silent accept.
      2. *Unintended* — the mode now applies to **every** directory the call creates, not just the
         leaf. With `auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is now
         `0700` and owned by the daemon user; previously parents took the process umask.
- [ ] **Log targets still name the crate the code left** — `tddy_daemon::auth`,
      `tddy_daemon::codex_oauth`, `tddy_daemon::github_token_store`, `tddy_daemon::oauth_tunnel`,
      `tddy_daemon::common_room`, `tddy_daemon::livekit_peer_discovery::peer_metadata`. **Kept
      deliberately**: renaming them would silently break every operator's `RUST_LOG` filter, and node
      1 set the same precedent (`tddy-host-service` still logs `tddy_daemon::host_private_key`). A
      fleet-wide rename is its own change with its own release note
- [ ] **The dependency-boundary harness is duplicated** — ~100 identical lines in
      `tddy-daemon-auth/tests/dependency_boundary_unit.rs` and its `tddy-daemon-livekit` twin, and
      nodes 5–8 will each add another copy. Lift into `tddy-testing-commons`. It also has two latent
      blind spots, both harmless against today's tree: a table-form dependency
      (`[dependencies.tddy-daemon]` with `path` on its own line) is skipped, and a workspace-inherited
      path dep (`{ workspace = true }`) is never followed
- [ ] **The four Docker-backed LiveKit suites now contend.** They are `#[serial]` *within* a binary,
      but cargo parallelises binaries, and they are now 4 of 8 in a small crate rather than 4 of 25 in
      `tddy-daemon`. One whole-crate run lost a test to container contention; `--test-threads=1`
      across all four is 11/0. Wants a crate-spanning `serial_test` group
- [ ] **`packages/tddy-web/src/buildId.ts` is a committed build artifact** that regenerates on every
      build, so it is diff noise on every PR in this stack and a guaranteed conflict on every cascade
      rebase

## Baseline

| Gate | Before | After |
|---|---|---|
| `cargo test -p tddy-daemon --no-fail-fast` | **1027 passed / 1 failed**, 25 suites (inherited from node 1) | **1600 passed / 20 failed / 3 ignored** — see below |
| `cargo test -p tddy-daemon --lib` | — | **466 passed / 0 failed** |
| `cargo test -p tddy-daemon-livekit` | — | **99 passed / 0 failed**, 8 suites |
| `cargo test -p tddy-service` + `tddy-daemon-auth` | — | **177 passed / 0 failed** |
| `cargo test -p tddy-workflow -p tddy-daemon-kernel` | — | **91 passed / 0 failed** |
| `cargo test -p tddy-livekit` | — | **66 passed / 0 failed** |
| `cargo clippy` over **all six touched crates** `--all-targets -- -D warnings` | ✅ exit 0 | ✅ exit 0 — after fixing `items_after_test_module`; the original two-crate command never covered `tddy-daemon-auth` |
| `cargo fmt --check` | ✅ | ✅ |
| `scripts/generated-code.sh check` | ✅ | ✅ (with `livekit_pb.ts` added) |
| LiveKit suites with `/var/run/docker.sock` absent | never skipped — they **fail**; see `## Scope` | unchanged; 11 ran green with Docker present |

**The recorded "1 failed" baseline is not what this machine produces, and the difference is not
this node's.** Of the 20 failures, **18 are the sandbox family** and none touches LiveKit: 17 are
`ConnectionServiceImpl::self_arc called before set_self_handle`
(`svc_resolve_tddy_tools_path.rs:161`) across `sandbox_behavior_acceptance`,
`sandboxed_claude_cli_acceptance`, `sandboxed_cursor_cli_acceptance`,
`sandboxed_session_lifecycle_acceptance`, `sandbox_session_stdio_acceptance` and
`cursor_cli_session_acceptance`, and one is `sandbox-runner spawn argv must pass --stdio`.
`set_self_handle` is called from `runtime.rs:821` and from nowhere this node changed — the diff
does not contain the string. These belong to node 3's territory (`tddy-daemon-sandbox`) and are
recorded here, not fixed.

⚠ **One contention change the move does cause, and it is in the harness, not the code.** The four
Docker-backed suites are `#[serial]` *within* a binary, but cargo runs binaries in parallel and all
four now sit in a crate with only eight, where before they were four of `tddy-daemon`'s twenty-five.
A whole-crate `cargo test -p tddy-daemon-livekit` therefore starts them closer together against one
shared LiveKit container, and `session_room_livekit_acceptance` lost one test to that on one run of
three. Run alone it is **6 passed / 0 failed**, and `--test-threads=1` across all four is
**11 passed / 0 failed**. Worth a `serial_test` group spanning the crate, or a note in the crate's
README — recorded, not fixed, because it is a harness property rather than a behaviour one.

The other **2 daemon-side failures are timing flakes that pass on re-run** of the same tree:
`relay_idle_wired_acceptance::rpc_call_bumps_idle_tracker_so_shutdown_is_not_triggered` (a 1 ms
idle timeout read immediately after the call, so any scheduling hiccup between the bump and the
read fails it) and `sandbox_runner_stdio_acceptance` ("expected non-empty PTY output").

Two further things a reader re-running this will hit, neither a regression:

- **`cargo test -p tddy-daemon` needs binaries `cargo test` does not build.** 19 failures in the
  first full run were `tddy-remote-git-repo is not built`, `build tddy-coder` and
  `build tddy-demo-tui` after a `./clean`. `./test` builds them first; plain `cargo test` does not.
  With them built, `remote_git_livekit_acceptance`, `session_sync_livekit_acceptance` and
  `session_agent_remote_acceptance` are **40 passed / 0 failed**.
- **`connection_service::stack_child_spawn_tests::a_child_started_from_the_dialog_is_told_to_read_its_changeset`
  is flaky.** It failed once and passed on two re-runs of the same tree. It reads
  `the_agent_was_spawned_with()`, a record its sibling test writes too, so the two race under the
  default thread count. Pre-existing, and belongs to whoever owns the PR-stack spawn tests

**9 failing tests** define this node: 3 in `tddy-daemon-auth` (one of them the ⛔ atomic-write
prerequisite), 3 in `tddy-daemon-livekit`, and 3 in `tddy-service` — the two inherited from node 1
plus one asserting `StreamLiveKitRooms` has actually left `connection.ConnectionService`.

**The draft surface lost to the real one, in four places.** The draft's shapes were placeholders
and the moved code's are what bind:

| Draft | Real | Why |
|---|---|---|
| `build_livekit_entry(Arc<CommonRoomPeerRegistry>)` | `build_livekit_entry(Arc<dyn RoomRoster>, SessionUserResolver)` | the rooms panel reads the LiveKit **server's** roster, which is `RoomRoster`; `CommonRoomPeerRegistry` is peer *eligibility* and answers a different question. The resolver is how the stream authenticates |
| `LiveKitError::{NotConfigured, Unreachable}` | `livekit_rooms_stream::RosterError::{Unconfigured, ReadFailed}` | already existed, already carries the distinction, and already maps it to two gRPC codes. A second error type beside it would be the "do not create a second type" the contract forbids |
| `SessionRoomRegistry::ensure(&str)` | `SessionRoomRegistry::open` / `ensure_open(&SessionRoomHosting, S, &dyn SessionTerminalBridge)` | a room is opened *for a hosting*, and single-flight is the point (`Self::openings`). The draft's `ensure` had no way to express either |
| `CommonRoomPeerRegistry::peers() -> Vec<String>` | `snapshot_remotes() -> Vec<EligibleDaemonInfo>` | a peer is a routable daemon, not a name |

The four draft tests were kept and retargeted rather than dropped, except
`opens_one_room_per_session_and_reuses_it`: reuse needs a LiveKit server, which is
`session_room_livekit_acceptance.rs`. In its place `names_one_room_per_session_and_never_two` pins
the same property where it is actually decided — the room name is a function of the session id
alone, so a second room for one session is not something either daemon can produce.

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
