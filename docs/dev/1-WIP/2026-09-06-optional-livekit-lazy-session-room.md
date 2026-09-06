# Changeset: optional-livekit-lazy-session-room

**Stack:** `optional-livekit` — node 9 of 9 (parent: `livekit-disable`, PR base
`feature/optional-livekit/livekit-disable`)
Discovery: [`2026-09-06-optional-livekit-lazy-session-room-initial-discovery.md`](2026-09-06-optional-livekit-lazy-session-room-initial-discovery.md)

## State A

- The facilitating daemon opens a session's LiveKit room **before spawning the agent**
  (`open_session_room_before_spawning_agent`), which `docs/ft/daemon/session-room.md` states as the
  contract.
- LiveKit **absent** → `open_measured_by` returns `Ok(None)` and the session starts fine.
- LiveKit **configured but unreachable** → `create_room` blocks with no timeout at any layer and the
  failure becomes `Status::internal`, killing session start. Reproduced in production.
- The PTY bridge in the same session start already tolerates a LiveKit failure with a `warn!`, so the
  two halves of one operation disagree about whether LiveKit is load-bearing.

## State B

- **Session creation never touches LiveKit.** For any session type, configured or not, reachable or
  not. Starting a session is a local operation: worktree, branch, changeset, agent.
- The session room **and the PTY bridge** are created **on the first connection to LiveKit** — an
  idempotent `ensure` at each point of use, rather than eagerly at spawn. The bridge is what lets a
  *remote* client drive the terminal over LiveKit; the desktop's own host is reached over IPC and
  needs no bridge at all, so a desktop-only deployment creates neither.
- A session created while LiveKit was down works fully once LiveKit is reachable, with no restart.
- LiveKit control-plane calls are **bounded**, so an unreachable server fails fast and legibly
  instead of hanging until a client gives up.
- Everything that has a session room today still has one, at the moment it is first needed.

## Responsibility

- Removing every LiveKit interaction from the session-start path — the eager room open **and** the
  eagerly-spawned PTY bridge.
- An idempotent `ensure_session_room` used by **every** consumer that needs the room — `ConnectSession`,
  split-placement start, `ensure_session_room_for_agents`, session-sync mirroring, seeded agent
  clones, participant admission, and the split agent's remote-identity route (discovery lists the
  call sites).
- Concurrency: two simultaneous first-connections must yield one room, not two.
- A timeout on the LiveKit control-plane calls (`room_metadata.rs` has none).
- Amending `docs/ft/daemon/session-room.md`, whose contract says the room opens before the agent is
  spawned — this node changes that.
- Rewording `connection_service.rs:12617`'s comment, which is about the *terminal* room and has
  already caused one wrong design.

## Boundaries

- Does **not** gate room creation on session type. `claude-cli` and `cursor-cli` run agents and their
  session rooms are a documented feature; the variable is reachability, not type. An earlier attempt
  at this node took that path and was stopped before any code was written.
- Does **not** make a LiveKit failure silent at the point a client asked for LiveKit. A connect that
  needs a room and cannot get one must fail loudly. Only *session creation* is unblocked.
- Does **not** touch `auth.rs:63`'s `signing_secret` or `runtime.rs:741`'s socket secret.
- Does **not** add or read a config flag — the explicit disable is node 8's.
- Does **not** change the common room, peer discovery, or the terminal room.
- Does **not** fix `connection_service.rs:10713`'s `"LiveKit not configured"` precondition on the
  `tool` session path — real, narrower than it looks, and separate.
- Adds **no dependency**.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `livekit-disable` (#449) | `livekit.enabled` and the single accessor the join paths delegate to | nothing directly — this node is orthogonal and sits above it only because the stack is linear | add to, read, or change the `enabled` flag or its accessor |

**Sequencing:** no real dependency on #449. It sits here because `gh stack` is linear and both are
daemon-side, so keeping them adjacent minimises conflict surface.

## Draft PR contract

Lands first:

1. The `ensure_session_room` signature and the bounded control-plane call.
2. Failing tests: `StartSession` contacts LiveKit **zero** times with LiveKit configured, and again
   with it absent — asserting the *absence of the call*, not merely a successful return.
3. A failing test that a session created while LiveKit is unreachable still starts.

## TODO

- [x] Record initial discovery
- [ ] Create PRD documentation
- [ ] Create changeset — this file
- [ ] Create failing acceptance tests
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red
- [ ] Implement (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Risks

- **Deferring breaks a documented invariant.** The PRD calls the facilitating daemon the room's first
  participant, opening it before the agent is spawned. Lazy creation changes *when*, and must not
  change *whether*. If an agent can reach the room before the daemon has joined it, that is a real
  regression and the design has to answer it rather than the changeset waving at it.
- **A missed consumer means a missing room** where today one is guaranteed. Discovery lists six; the
  implementation must prove that list complete rather than trust it.
- **Silent degradation.** The temptation is to make room-open failures non-fatal everywhere. That is
  explicitly out of bounds: it would cost a session its sync, clone and split capability with only a
  log line to show for it.

## Commands

```bash
./dev cargo test -p tddy-daemon           # note: sandbox_behavior_acceptance fails on master already
./dev cargo test -p tddy-livekit
./dev cargo clippy -p tddy-daemon --all-targets -- -D warnings
./desktop-dev                             # manual: create a session with LiveKit unreachable
```
