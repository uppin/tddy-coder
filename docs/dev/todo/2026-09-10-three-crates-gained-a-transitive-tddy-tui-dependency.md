# 2026-09-10 — Three crates gained a transitive `tddy-tui` dependency, all from `tddy-service`

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M6 / M7 / M8

`tddy-service` depends on `tddy-tui`. Node 5 moved three module groups out of `tddy-tools` into
crates that had to acquire `tddy-service` to carry the protos those modules speak, so each one now
builds the TUI transitively:

| Milestone | Crate | Why it needed `tddy-service` |
|---|---|---|
| M6 | `tddy-terminal-rpc` | `pty_relay` encodes `connection.ConnectionService` and `auth.AuthService` messages |
| M7 | `tddy-session-tool-client` (new) | every message `dispatch_session_tool` sends is a `tddy-service` proto |
| M8 | `tddy-discovery` | `roster::registry` names `AgentCloneState`, `SessionAgentEntry`, `SessionAgentRoster` from `connection` |

The root cause is recorded from node 4 in
[`2026-09-10-tddy-service-depends-on-tddy-tui-so-every-service-crate-builds-the-tui.md`](./2026-09-10-tddy-service-depends-on-tddy-tui-so-every-service-crate-builds-the-tui.md);
this entry adds node 5's three crates, their blast radius, and the one change that closes both.

None of the three was avoidable by a different destination — the changeset proves each one (see
`## Boundaries`), and in M8's case the five `session_agents` modules provably cannot split.

**The cost is not uniform.** `tddy-terminal-rpc`'s three reverse-dependencies (`tddy-daemon`,
`tddy-coder`, `tddy-tools`) all depend on `tddy-service` directly already, so it costs nothing
measurable. `tddy-discovery` has **eight** (`tddy-acp`, `tddy-coder`, `tddy-daemon`,
`tddy-daemon-kernel`, `tddy-model-registry`, `tddy-sandbox-app`, `tddy-sandbox-runner`,
`tddy-spawn`), and the in-jail ones now build a terminal UI in order to follow an agent roster.

**All three are fixed by the same change: split the generated protos out of `tddy-service` into a
crate that depends on nothing.** It is the same fix as
[`tddy-livekit`'s `tddy-service` dependency](./2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites.md),
and the two should be done together.

Deferred because it is a change to `tddy-service`'s public shape, which node 5's `## Boundaries`
excludes outright — it touches no proto.
