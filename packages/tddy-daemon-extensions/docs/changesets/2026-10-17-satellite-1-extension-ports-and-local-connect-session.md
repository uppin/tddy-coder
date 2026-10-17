# 2026-10-17 — The extension-ports crate

**Type:** Architecture

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

The crate is created. It holds the session-reach port (`SessionReach`, `ReachableSessions`,
`ReachedSession`, the null object `NoSessionReach`) and the integration status board
(`IntegrationStatusBoard`, `IntegrationReporter`, `IntegrationState`, `IntegrationStatus`). It depends
on `tddy-rpc` and `async-trait` and on nothing that reaches a LiveKit crate.

- Board semantics: a reporter registers its integration as `Disabled`; registering an id again keeps
  its entry; a report of the same kind of state, whatever its detail, keeps `since`.
- `tests/livekit_is_a_satellite.rs`, the ratchet: four tests over `WorkspaceGraph`. Lists at this
  point: `PLUGINS` 5, `WIRING` 4, `KNOWN_OFFENDERS` **18** (`tddy-agent-launch`, `tddy-cli-sessions`,
  `tddy-coder`, `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-rpc`, `tddy-host-service`,
  `tddy-remote-git-repo`, `tddy-screen-sharing`, `tddy-session-activity`, `tddy-session-agents`,
  `tddy-session-files`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-session-sync`,
  `tddy-session-tool-client`, `tddy-telegram-control`, `tddy-terminal-rpc`). The list only shrinks.
- No `docs/code-issues/` record: the crate is small and unanalysed rather than known clean.
- Docs: [extension-ports.md](../extension-ports.md), [README](../../README.md).
