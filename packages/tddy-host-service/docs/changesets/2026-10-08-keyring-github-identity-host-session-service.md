# 2026-10-08 — `HostSessionService` answers `GithubToken` through a session registry

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`HostSessionService` gains the `GithubToken` method beside `SpawnConversation`; every request names its
`session_id`. `HostSessionRegistry` maps session id to `RegisteredSession { os_user, handlers }` with the pid
of the process it was last started as. Wrong-OS-user requests are refused without naming either user, an
unregistered session that exists on disk is told to resume, and `unregister_stopped` removes an entry only
while its pid is still the registered one.

Detail: [host-session-service.md](../host-session-service.md).
