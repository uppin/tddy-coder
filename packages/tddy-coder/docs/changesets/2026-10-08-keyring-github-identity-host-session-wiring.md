# 2026-10-08 — A tool session's listener relays the token to the daemon

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`tool_host_wiring::ToolHostHandlers::from_flags(socket, session_id)` binds `DaemonRelayConversationSpawnHandler`
and `DaemonRelayGithubCredential` exactly when both are present; `HostSessionClient` reconnects after a lost
connection and never repeats a spawn. `run.rs` seeds the presenter's `github_pr_tools_available` from the
same fact.

Detail: [host-session-wiring.md](../host-session-wiring.md).
