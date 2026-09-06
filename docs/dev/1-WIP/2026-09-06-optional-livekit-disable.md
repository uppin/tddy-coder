# Changeset: optional-livekit-disable

**Stack:** `optional-livekit` — node 8 of 8 (parent: `desktop-ipc-host`, PR base
`feature/optional-livekit/desktop-ipc-host`)
PRD: [`2026-09-06-optional-livekit-disable-prd.md`](2026-09-06-optional-livekit-disable-prd.md)
Discovery: [`2026-09-06-optional-livekit-disable-initial-discovery.md`](2026-09-06-optional-livekit-disable-initial-discovery.md)

## State A

- LiveKit is optional only in the sense of *absent*. Nodes 1–7 made every surface work without it:
  a pluggable connection registry, a source-merged host directory, capability-gated media and
  presence, and the desktop's own host over IPC.
- "Is LiveKit usable" is decided in **eight** places (discovery lists them), one canonical —
  `CommonRoomTarget::from_livekit` — and seven re-derivations with differing field sets.
- Turning LiveKit off means deleting the `livekit:` block. There is no flag, and no UI affordance.
- `livekit.api_secret` doubles as the **session-token signing secret** (`auth.rs:63`).

## State B

- `livekit.enabled` exists, defaults to **`false`**, and is edited from the Settings screen.
- Disabled: no common room joined, no peer metadata published, no common-room token minted, and the
  web app constructs no `Room`. The directory reports `idle`, not `error`.
- Enabled: unchanged from today.
- The API secret still signs session tokens either way, so a disabled daemon still authenticates its
  own gated RPCs — including the one that turns LiveKit back on.
- Per-session rooms, screen sharing and the `#/livekit` rooms panel are untouched.

## Responsibility

- `livekit.enabled` on `LiveKitConfig`, its serde default, its `Default` entry, and **one accessor**
  the join paths delegate to instead of an eighth re-derivation.
- The daemon honouring it: no target from `CommonRoomTarget::from_livekit`, no peer-discovery
  assembly, no advertisement, no common-room token from either mint.
- The supervisor saying *disabled* rather than *incomplete* — different operator problems.
- `enabled` on `LiveKitSettings` (field **7**) and the disabled state on `GetClientConfigResponse`
  (field **8**), plus the three `ClientConfig` construction sites kept in agreement.
- `common_room_changed` accounting for the flag, so saving the toggle actually disconnects.
- The web short-circuit, and the Settings screen toggle with its test id and driver support.
- Tests for all of it, and the docs that become wrong.

## Boundaries

- Does **not** touch `auth.rs:63`'s `signing_secret` or `runtime.rs:741`'s socket secret. Disabling
  LiveKit must never disarm daemon authentication — that is the lockout this node exists to avoid.
- Does **not** change what happens when LiveKit is *enabled*. This is an off switch only.
- Does **not** govern per-session rooms (`spawner.rs:1437`), screen sharing
  (`screen_sharing_service.rs:232`), `session_room.rs`, or the `#/livekit` rooms panel
  (`livekit_rooms_stream.rs`). They read the same block for other purposes and keep working.
- Does **not** add a per-host or per-browser override — this is the serving daemon's setting.
- Does **not** rewrite the seven existing re-derivations beyond pointing them at the new predicate.
- Does **not** change capability gating (node 4's) or the host-directory merge (node 2's).
- Adds **no npm and no Rust dependency**.

## Dependencies

What the parent PR delivers that this one consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `desktop-ipc-host` (#443) | and through it nodes 1–6: the `ConnectionProvider` registry, the source-merged host directory with its idle-not-error contract, capability gating, and the desktop's own IPC host so a daemon with LiveKit off is still reachable | the disabled path relies on the local host remaining fully usable, and on `liveKitSource` already reporting `idle` from `useCommonRoom`'s guard | add to `ConnectionCapability`, change the directory merge or its ordering, change `useHasCapability`, or touch `localHost.ts` / `localHostRegistration.tsx` |

**Sequencing:** this node needs #443 only for the guarantee that a daemon with no common room is still
a working app. The daemon half depends on nothing in the stack and could be written first.

## Draft PR contract

Lands first:

1. `livekit.enabled` on `LiveKitConfig` with its default and accessor, and the proto fields (7 and 8),
   so both halves have a real signature to code against.
2. Failing tests for the daemon half: no target when disabled, no advertisement, no mint, and — the
   important one — **a gated RPC still authenticates with LiveKit disabled**.
3. Failing tests for the reconnect gap: saving the toggle disconnects a live common room.

Implementation lands in the same PR. **Not a merge candidate on the contract alone.**

## TODO

- [x] Record initial discovery
- [x] Create PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — failing unit/integration tests
- [ ] Implement production code (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Risks

- **The lockout.** Reading "disabled" as "the block is absent" disarms `signing_secret` and refuses
  every gated RPC, including the one that re-enables it. AC5 exists solely to pin this.
- **The silent toggle.** `common_room_changed` keys on `(url, common_room)`; miss it and the toggle
  saves while the room stays connected. `keeps_the_common_room_connected_when_an_unrelated_field_changes`
  is the test that would wrongly pass.
- **The default flip.** `enabled: false` by default disconnects every existing deployment on upgrade.
  Intended and operator-approved, but it is a breaking change and the changelog must lead with it.
- **`deny_unknown_fields`.** A daemon older than the field rejects a config the newer UI wrote.

## Commands

```bash
./dev cargo test -p tddy-daemon
./dev cargo test -p tddy-core
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
./desktop-dev                          # manual, BOTH states of the toggle
```
