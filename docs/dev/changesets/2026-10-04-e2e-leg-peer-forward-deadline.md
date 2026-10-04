# 2026-10-04 — The peer-forward deadline is a daemon setting; `rpc_scenarios` is one test per scenario

**Type:** Feature (daemon setting) + Refactor (test split)

`#e2e-leg` 2/5, PR [#579](https://github.com/uppin/tddy-coder/pull/579) (`feature/e2e-leg/deadline-and-scenarios`).
Product entry: [2026-10-04-peer-forward-deadline-setting.md](../../ft/daemon/changelog/2026-10-04-peer-forward-deadline-setting.md).
Package entries: `tddy-daemon-kernel`, `tddy-daemon-livekit`, `tddy-host-service`, `tddy-session-agents`,
`tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-livekit`, `tddy-daemon` (each `docs/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md`).

## What changed

`PEER_FORWARD_TIMEOUT` (30 s) is the default of `DaemonConfig::peer_forward_timeout_secs`; the accessor
clamps to 1 s. The deadline travels with a `CommonRoom` handle (room slot plus deadline) that replaced the
bare room slot at about 24 call sites across six crates; the three forwarders are its methods. The stream
**open** deadline follows the setting, the idle timeout stays fixed, and `refuse_departed_daemon` is
unchanged. The silent-peer deadline test sets 2 s and still asserts `DeadlineExceeded`. `rpc_scenarios` is
one test per scenario. `daemon.yaml.production` carries the commented key. Permanent docs:
[daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md#peer_forward_timeout_secs-and-commonroom--how-long-a-forward-to-a-peer-waits),
[livekit-peer-discovery.md](../../ft/daemon/livekit-peer-discovery.md),
[remote-managed-worktree.md](../../ft/daemon/remote-managed-worktree.md).

## Decisions

- **Name `peer_forward_timeout_secs`**, matching the other durations on `DaemonConfig`.
- **The deadline rides with the room slot** (developer's decision): one type change at the seam, no
  global (the e2e tests run several daemons in one process), no ~25 new arguments.
- **Idle timeout stays fixed**; it is pinned by the roster's service-threshold test and a compile-time assert.
- **Slot-taking constructors kept**: `PeerRouting::new`, `HostServiceImpl::with_common_room` and
  `LiveKitEligibleDaemonSource::new` still take the slot and build the `CommonRoom` from their own config,
  which avoids about 25 test files constructing the discovery handles.
- **Two integration tests edited for the API change, assertions unchanged:**
  `forward_to_peer_shared_registry` and `forwarded_rpc_is_stamped_by_the_receiver`.
  `SessionAgentCloneSpec.common_room_slot` is renamed `common_room`.
- **`rpc_scenarios`:** each test starts its own LiveKit handle (sharing one server is `#e2e-leg` 4/5 and
  5/5); one timeout message in the stateful-bidi test is reworded, assertions unchanged.
- The 12 s negative window in `common_room_set_metadata_handshake_repro` is left alone: it is the property under test.

## Backlog

`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits` is **narrowed, not deleted**: this node
delivers step 4 (section 2) of five; the shared server, the `docker` group and the compile share remain.

## Code issues (re-measured, production lines to the first `#[cfg(test)]`)

| Record | Before | After | Outcome |
|---|---|---|---|
| `oversized-file-config` (`config.rs`) | 1,476 | **1,491** | regressed +15; **decomposition deferred with developer consent** (relayed by the coordinator); stays open |
| `heavy-dependency-livekit-peer-forwarding` (`peer_forwarding.rs`) | 266 | 304 | unchanged class; stays open |
| `oversized-file-livekit-peer-discovery` | 1,638 | 1,636 | not grown; stays open |
| `oversized-file-service` (host-service) | 939 | 934 | not grown; stays open |
| `oversized-file-session-agent-clone` | 1,158 | 1,157 | not grown; stays open |

No node of the stack (#580-#582) touches `config.rs`, so the deferral is a consent, not the stack rule; the split
belongs in its own PR.

## Validation (scoped to the touched packages, run locally on macOS)

- `cargo fmt -p` on the seven touched crates: clean. `cargo clippy -D warnings` on `tddy-daemon-kernel`,
  `-daemon-livekit`, `-host-service`, `-session-agents`, `-session-lifecycle`, `-daemon-rpc`: clean.
- `cargo test --no-fail-fast` over `tddy-daemon-kernel`, `-daemon-livekit`, `-host-service`,
  `-session-agents`, `-daemon-rpc`: **753 passed, 0 failed.** 14 `tddy-daemon-rpc` jail tests first failed
  only because `tddy-tools` / `tddy-sandbox-runner` were not built (`./test` builds them); rebuilt, all pass.
- `tddy-livekit --test rpc_scenarios`: 11 passed (46.8 s). `tddy-daemon --test session_attach_cross_host_acceptance`:
  8 passed, including the silent-peer deadline test.
- `tddy-session-lifecycle --lib --test remote_managed_worktree_acceptance`: 150 + 19 passed.
- **Deferred to CI — verified by the PR checks, not a local gate:** the rest of the `tddy-session-lifecycle`
  integration suites and the rest of the `tddy-daemon` suites (compiled only), and the e2e leg.
