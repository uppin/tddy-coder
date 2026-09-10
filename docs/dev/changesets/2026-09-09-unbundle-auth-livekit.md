# 2026-09-09 — The identity boundary and the LiveKit subsystem, each in its own crate

**Type:** Architecture

Node 4 of the `#unbundle` stack ([#473](https://github.com/uppin/tddy-coder/pull/473), 4 of 8, based
on node 3's branch). Two subsystems leave `tddy-daemon` in one PR: the identity boundary, and
everything LiveKit.

They are unrelated subsystems and under a 25-node plan they were two PRs. They are one here because
of the consolidation to eight — and the pairing is the least arbitrary available, because they
share the one secret that signs both LiveKit room JWTs and session tokens. A reviewer of this node
sees that fact once; a reviewer of either half alone would not see it at all.

## What landed

| | |
|---|---|
| `tddy-daemon-auth` *(new)* | 7 modules, 2,145 prod LoC, serving `auth.AuthService` (5), `auth.LiveKitTokenService` (1), `token.TokenService` (2), `loopback_tunnel.LoopbackTunnelService` (1) — **all already their own protos** |
| `tddy-daemon-livekit` *(new)* | 4 moved modules and 1 authored, 6,730 prod LoC, serving **`livekit.LiveKitService`** |
| `tddy-daemon` | 11 modules and 8,875 prod LoC leave; the common-room supervisor task leaves `RuntimeTasks` |
| `tddy-service` | `livekit.proto` appears; `connection.proto` **73 → 72** rpcs, minus the 12 messages that travel with the method |
| `tddy-web` | the rooms panel and its Cypress fake address `livekit.LiveKitService`; `livekit_pb.ts` regenerated |
| `tddy-daemon-kernel` | +17 lines — `SPLIT_AGENT_IDENTITY_PREFIX`, cycle cut A |
| `tddy-workflow` | +57 lines — `SessionAttachmentFile` and `list_session_attachments` in `artifact_paths`, cycle cut C |
| `tddy-integration-tests` | reaches `codex_oauth_relay` through `tddy-daemon-auth`; its `tddy-daemon` dependency is **gone entirely**, not merely joined |
| `tddy-livekit`, `tddy-rust-typescript-tests` | two call sites repoint to `proto::livekit`; the regenerated `livekit_pb.ts` |

Docs: [`tddy-daemon-auth`](../../../packages/tddy-daemon-auth/docs/auth-service.md),
[`tddy-daemon-livekit`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md),
[`session-room.md`](../../../packages/tddy-daemon-livekit/docs/session-room.md),
[`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md),
[`livekit-rooms-panel.md`](../../ft/web/livekit-rooms-panel.md),
[`host-worktree-services.md`](../../ft/daemon/host-worktree-services.md).

## One RPC coordinate moved

`StreamLiveKitRooms` left `connection.ConnectionService` for **`livekit.LiveKitService`**, and
`packages/tddy-web`'s rooms panel migrated in the same change. Everything else on the wire is
unchanged: auth's four services were already their own protos, so no client migrated for the larger
half of this node.

**A web bundle and a daemon must be from the same side of the split** for that one panel — the
repo's standing policy of breaking freely and migrating every consumer in the same change.

The round-trip is proven rather than merely compiled. The Cypress fake registers its handler **only**
on `LiveKitService`, and the testkit router answers any unregistered method with
`Code.Unimplemented`, so a client still asking `ConnectionService` would render the error branch and
fail the suite. `LiveKitRoomsPanelAcceptance.cy.tsx`: 26/26. The daemon side asserts by
**dispatching**, not by reading a name list — `livekit.LiveKitService` answers `Unauthenticated`
(handler reached, token judged) while `connection.ConnectionService` answers `NotFound` (handler
gone, not merely refused).

`livekit.proto` turned out to be a third self-contained cut: its closure is **12 messages with zero
overlap** with anything that stays, so like `host.proto` and `worktree.proto` it imports nothing.
There is still no `types.proto`, and this node did not need one.

## Four planning premises were wrong, and the corrections are the interesting part

- **`n2 → n4` was not a real edge.** The plan said node 4 had to wait on node 2, because the
  `session_room` consumer was assumed to be inside `tddy-telegram` by then. It is not:
  `telegram_session_control.rs` holds `SessionRoomRegistry` as a struct field and **stays in
  `tddy-daemon`** — it orchestrates the session lifecycle through 12 daemon-owned modules and sits
  in two real production cycles, so it belongs to a *successor* of nodes 4 and 6–8. The repoint
  happened in place, `tddy-telegram` needed no manifest change, and the true waves are **w1** `n1`;
  **w2** `n2`, `n3`, `n4`, `n5`; **w3** `n6`, `n7`, `n8`.
- **Four cycle cuts, not two.** Node 1's audit assigned two to this node. A third —
  `session_room → session_attachments` — and a fourth — `livekit_peer_discovery →
  oauth_loopback_tunnel` — were found while making the move. The fourth is not a cycle at all but a
  forbidden direction, which a pair-based audit cannot see.
- **`multi_host.rs` is not this node's.** The plan named it as one of five LiveKit modules; node 1
  took it to `tddy-host-service` with `host_registry`. So **four** modules moved, and
  `livekit_peer_discovery ⇄ multi_host` needed no cut at all — nothing in `tddy-host-service` names
  a LiveKit module, so the edge runs one way.
- **5 of 18 test suites were movable, not 18.** Thirteen are joint session/LiveKit suites reaching
  `connection_service`, `test_util`, `split_session`, `claude_cli_session` or
  `session_attachment_staging`, every one of which stays for nodes 6–8. A suite that names them
  cannot move without putting `tddy-daemon` back on this crate's dependency path, which is the one
  thing the split exists to prevent. `session_room_acceptance.rs` and
  `session_room_cross_host_acceptance.rs` travel with the session families instead.

## The four cycle cuts, and why each went the direction it did

Rust crates cannot be mutually dependent, and no new crate may reach `tddy-daemon` at all. Each of
these spans a crate boundary only because this node moves one end of it — which is why node 1
assigned them to "the nodes that move those modules" rather than cutting them itself.

**A — `livekit_peer_discovery` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX`.** The constant is
lifted to `tddy_daemon_kernel::daemon_identity`, whose stated charter is already "the identity names
several crates need", with `split_session` re-exporting it so no caller path changed and there stays
one definition. The other direction — defining it in the LiveKit crate and having `split_session`
read it — would make the **producer** of the identity depend on the crate that *refuses* it, and
would break again when nodes 6–8 move `split_session` to a third crate.

**B — `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery`.** The three-hop
loop node 1's pair-based audit could not see. The `CommonRoomSupervisor` **trait** moved to
`common_room_supervisor.rs`, its one implementation, with `daemon_config_service` re-exporting it.
Verified rather than assumed: re-pointing the return leg alone does **not** cut it, because the
trait import is a `livekit → daemon` edge whether or not it closes a loop. The return leg was
re-pointed too, from a re-export in a departing module to its real home,
`tddy_daemon_kernel::daemon_identity::local_instance_id_for_config`.

**C — `session_room` → `session_attachments::list_session_attachments`.** `session_attachments`
belongs to the session family and stays. The listing is lifted to `tddy_workflow::artifact_paths`,
beside the `session_attachments_root` that names the very directory it reads, and the daemon
re-exports it. **Not the kernel**: this is a filesystem convention `tddy-workflow` already owns, not
a daemon identity.

**D — `livekit_peer_discovery` → `oauth_loopback_tunnel`.** Not a cycle, but a forbidden direction:
the LiveKit crate must not reach the identity boundary. `spawn_oauth_loopback_tunnel` moved to
`tddy_daemon_auth::oauth_loopback_tunnel`, the module whose supervisor it starts and whose own doc
already described the eligibility gate. `spawn_common_room_discovery_task`, which composes that
supervisor with the discovery loop, moved to **`tddy-daemon`'s `runtime.rs`** — it has no production
caller, and after the split the daemon is the only crate holding both halves.

Two of the four land in crates this node did not plan to touch (`tddy-daemon-kernel`,
`tddy-workflow`). Both **add** rather than modify, and both are called out here because a kernel
change inside a LiveKit PR is surprising to a reviewer who does not know why.

## One secret signs two things

`config.livekit.api_secret` signs **both** LiveKit room JWTs and session tokens, through
`tddy_github::SessionTokenSigner`. Session tokens are stateless and HMAC-signed on that key, so a
token minted by one daemon is verifiable by every daemon holding the same secret — that is how a
deployment shares an identity at all.

Splitting auth from LiveKit into two crates does **not** split that secret, and **neither crate may
start deriving its own**. A second signer would silently partition which tokens each half accepts,
and the partition would stay invisible until a cross-daemon call failed.

The enforcement is structural rather than a comment. `SessionTokenMinter` is a **port**, so
`tddy-daemon-livekit` never reaches minting directly, and its `dependency_boundary_unit.rs` has a
fourth test pinning `tddy-daemon-auth` off its dependency path — so the port cannot quietly stop
being one.

## The secret store writes atomically, and the test does not prove it

`github_token_store` was one of three secret stores recorded as truncating in place. It moves into
a crate whose whole purpose is to be the identity boundary, and every route around the defect while
*moving* it is wrong: relocating verbatim carries a known data-loss path into that crate, and
hand-rolling an atomic write beside `tddy_core::atomic_file::write_atomic` deepens the duplication
the entry is about. So the store was routed through the existing helper as part of the move — a
deliberate exception to "move only", on the grounds that the fix is a call rather than new
machinery.

**`write_atomic_with_mode`, not `write_atomic`.** The plain helper carries permission bits over from
an *existing* target, so a **first** write would create the swap file at the process umask and
publish a world-readable credential store.

⚠ **The test guards a future defect, not this change.** The base implementation already staged to
`<tokens>.tmp` and renamed, and that staged create also fails in a `0o555` directory — so reverting
the `write_atomic_with_mode` refactor would leave the test **green**. It is an honest guard against
a *future* truncate-in-place, and it does not discriminate this PR's change. Stated here rather than
left for a reader to discover.

⚠ **`ensure_owner_only_dir` changed behaviour twice, and only one half was deliberate.** It now
builds the directory with `DirBuilder::recursive(true).mode(0o700)` instead of `create_dir_all` plus
an unconditional `set_permissions(0o700)`:

1. *Intended* — an **existing** storage directory no longer has `0o700` re-imposed on every write,
   so the daemon stops overruling an operator's deliberate `chmod`. The cost is that an
   `auth_storage` which is currently group- or world-readable **stays that way after upgrade**,
   where before every `put` re-tightened it. That deserves a decision, not a silent accept.
2. *Unintended* — the mode now applies to **every** directory the call creates, not just the leaf.
   With `auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is created `0700` and
   owned by the daemon user; previously parents took the process umask.

The remaining two stores — `vnc_vault.rs` and `screen_sharing_vault.rs` — still truncate, and belong
to node 2. The
[todo entry](../todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md) is narrowed
to them rather than closed.

## Log targets deliberately still say `tddy_daemon::…`

`tddy_daemon::auth`, `tddy_daemon::codex_oauth`, `tddy_daemon::github_token_store`,
`tddy_daemon::oauth_tunnel`, `tddy_daemon::common_room`,
`tddy_daemon::livekit_peer_discovery::peer_metadata`.

A log target is an operator's `RUST_LOG` filter. Renaming these to match the crates the code is now
in would silently break every filter already selecting them — the failure mode being *missing logs*,
which is exactly the thing an operator turns to `RUST_LOG` to fix. Node 1 set the same precedent
(`tddy-host-service` still logs `tddy_daemon::host_private_key`). A fleet-wide rename is its own
change with its own release note.

## The Docker premise is inverted: nothing skips, they fail

The plan asked that the Docker-dependent LiveKit suites "skip exactly as they do today when the
socket is absent". They do not skip and never did. `LiveKitTestkit::start()` returns `Err` without
Docker and every caller `.expect(...)`s it, so an absent `/var/run/docker.sock` makes these suites
**fail**, loudly, with `LiveKit testkit (Docker, or LIVEKIT_TESTKIT_WS_URL)`.

Four such suites moved (`session_room_livekit_acceptance`, `livekit_peer_daemons_acceptance` and the
two common-room repros); `tddy-daemon-livekit` takes the same `tddy-livekit-testkit` dev-dependency
and the harness code is byte-identical, so the outcome is the same on both sides of the move.
Established by reading `LiveKitTestkit::start` rather than by unplugging Docker, and all 11 tests
ran green with Docker present.

⚠ **One contention change the move does cause, and it is in the harness, not the code.** The four
Docker-backed suites are `#[serial]` *within* a binary, but cargo runs binaries in parallel, and
they are now four of eight in a small crate where before they were four of `tddy-daemon`'s
twenty-five. A whole-crate run therefore starts them closer together against one shared LiveKit
container, and `session_room_livekit_acceptance` lost one test to that on one run of three. Run
alone it is 6/0, and `--test-threads=1` across all four is **11/0**.

## Baseline

| Gate | Result |
|---|---|
| `cargo build` — all 8 touched packages | ✅ 8/8 |
| `cargo test -p tddy-daemon --lib` | 466 / 0 |
| `cargo test -p tddy-daemon-livekit` | 99 / 0, 8 suites |
| `cargo test -p tddy-service` + `tddy-daemon-auth` | 177 / 0 |
| `cargo test -p tddy-workflow -p tddy-daemon-kernel` | 91 / 0 |
| `cargo test -p tddy-livekit` | 66 / 0 |
| `bun run --filter tddy-web cypress:component` — `LiveKitRoomsPanelAcceptance.cy.tsx` | 26 / 26 |
| `cargo clippy` over all six touched crates `--all-targets -- -D warnings` | ✅ |
| `cargo fmt --check` · `scripts/generated-code.sh check` | ✅ · ✅ (with `livekit_pb.ts`) |

**A scoped clippy gate must name every crate the node touches.** The first version of this node's
Code Quality line read `-p tddy-daemon-livekit -p tddy-service`, omitting `tddy-daemon-auth`, so an
`items_after_test_module` in `oauth_loopback_tunnel.rs` went unseen until `/pr-wrap`. CI runs
`--workspace --all-targets`, so anything narrower has to be complete by hand.

`cargo test -p tddy-daemon --no-fail-fast` reports 1600 passed / 20 failed / 3 ignored. **Eighteen
are the sandbox family and none touches LiveKit**: 17 are `ConnectionServiceImpl::self_arc called
before set_self_handle` across six suites, and one is a `sandbox-runner spawn argv must pass
--stdio` anchor. `set_self_handle` is called from `runtime.rs:821` and from nowhere this node
changed — the diff does not contain the string. They belong to node 3's territory. The remaining
two are timing flakes that pass on re-run.

Two things a reader re-running this will hit, neither a regression: `cargo test -p tddy-daemon`
needs binaries `cargo test` does not build (`./test` builds them; plain `cargo test` does not), and
`connection_service::stack_child_spawn_tests::a_child_started_from_the_dialog_is_told_to_read_its_changeset`
races a sibling test over a shared record.

## Files over budget

Five stand over 500 lines and **every one is relocated code, not new code**:
`session_room.rs` (2,992), `livekit_peer_discovery.rs` (2,060), `auth.rs` (1,200),
`common_room_supervisor.rs` (881), `livekit_rooms_stream.rs` (779). Everything this node **authored**
is under budget: `livekit_service.rs` 127, the two `lib.rs` 137 and 153.

Splitting them is deferred to a follow-up branch after the stack lands, on two independent grounds.
`session_room.rs` is reached by the session subsystem nodes 6–8 move, so a rename cascade through
their diffs turns each into a conflict. And `move_module_to_crate` only handles a whole
`<crate>/src/<module>.rs`, so every split has to happen either before the move — churning the diff a
reviewer reads as "did any logic change?" — or after it, at a second 40-minute rust-analyzer index
per file.

## Where the draft surface lost to the real one

The draft PR's shapes were placeholders, and the moved code's are what bind:

| Draft | Real | Why |
|---|---|---|
| `build_livekit_entry(Arc<CommonRoomPeerRegistry>)` | `build_livekit_entry(Arc<dyn RoomRoster>, SessionUserResolver)` | the rooms panel reads the LiveKit **server's** roster, which is `RoomRoster`; `CommonRoomPeerRegistry` is peer *eligibility* and answers a different question. The resolver is how the stream authenticates |
| `LiveKitError::{NotConfigured, Unreachable}` | `RosterError::{Unconfigured, ReadFailed}` | already existed, already carried the distinction, already mapped it to two gRPC codes |
| `SessionRoomRegistry::ensure(&str)` | `open` / `ensure_open(&SessionRoomHosting, S, &dyn SessionTerminalBridge)` | a room is opened *for a hosting*, and single-flight is the point; the draft's `ensure` expressed neither |
| `CommonRoomPeerRegistry::peers() -> Vec<String>` | `snapshot_remotes() -> Vec<EligibleDaemonInfo>` | a peer is a routable daemon, not a name |

## Open items

Recorded in `docs/dev/todo/`:
[no LiveKit RPC call has a client-side deadline](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md),
[the room-creation call has no timeout](../todo/2026-09-06-the-livekit-room-creation-call-has-no-timeout.md),
[the dependency-boundary harness is duplicated](../todo/2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md),
[the Docker-backed LiveKit suites contend](../todo/2026-09-10-the-docker-backed-livekit-suites-contend-across-binaries.md),
[`ensure_owner_only_dir` no longer re-tightens an existing auth_storage](../todo/2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md),
[`tddy-service` depends on `tddy-tui`](../todo/2026-09-10-tddy-service-depends-on-tddy-tui-so-every-service-crate-builds-the-tui.md),
[the moved modules are over the file budget](../todo/2026-09-10-the-auth-and-livekit-modules-are-over-budget-and-were-moved-unsplit.md),
[log targets still name `tddy_daemon`](../todo/2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md),
[`buildId.ts` is a committed build artifact](../todo/2026-09-10-buildid-ts-is-a-committed-build-artifact.md).

Narrowed rather than closed:
[the daemon's secret stores still truncate in place](../todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md)
— one of its three stores is fixed, two remain and are node 2's.

Also carried forward unchanged, as inherited defects now visible inside `tddy-daemon-livekit`:
[a split session's room is not re-opened when the daemon restarts](../todo/2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md),
[LiveKit connect/streamer duplication](../todo/2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md).
