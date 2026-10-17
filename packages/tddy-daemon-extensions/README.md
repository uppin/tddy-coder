# tddy-daemon-extensions

The extension ports the daemon's core exposes to optional integrations (LiveKit is the first), and
the status board those integrations report their live state on. The core depends on this crate; an
integration implements its ports; only the crates that build a binary bring the two together.

## Quick Start

### Test
```bash
cargo test -p tddy-daemon-extensions
```

## Architecture

The crate holds two things and nothing that reaches an integration: the **session-reach port**
(`SessionReach`, `ReachableSessions`, and the null object `NoSessionReach`), through which a reached
session is reported and an integration extends it beyond this host; and the **integration status
board** (`IntegrationStatusBoard`, `IntegrationReporter`, `IntegrationState`), the one place an
integration's live state is recorded. A workspace ratchet test holds the rule that no LiveKit crate
is reachable from here.

## Documentation

### Product Requirements (What)
- [Session rooms](../../docs/ft/daemon/session-room.md) — when a room is opened, and for whom
- [Daemon settings](../../docs/ft/daemon/daemon-settings.md) — the integration status an operator reads

### Technical Implementation (How)
- [Extension ports](./docs/extension-ports.md) — the ports, the status board and the ratchet
- [Changesets](./docs/changesets/) — applied changeset history

## Related Packages
- [tddy-daemon-livekit](../tddy-daemon-livekit/README.md) — the LiveKit implementation of the ports
- [tddy-agent-launch](../tddy-agent-launch/docs/module-layout.md) — `ConnectSession` reports its reach here
- [tddy-session-lifecycle](../tddy-session-lifecycle/README.md) — implements `ReachableSessions` on the session host
- [tddy-testing-commons](../tddy-testing-commons/docs/dependency-graph.md) — the workspace graph the ratchet reads
