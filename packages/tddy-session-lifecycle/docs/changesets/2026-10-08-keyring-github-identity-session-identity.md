# 2026-10-08 — Sessions act as the project's account; the token over a per-OS-user host-session socket

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

One `SessionAccountAccess::session_identity` lookup binds the commit pairs and a `SessionGithubCredential`
pinned to the start's outcome, on every session spawn path the daemon owns. A refused resolution still starts
the session, with no pairs. Tool sessions get `--host-session-socket` whatever the recipe: one `0600`
socket per OS user under a `0700` directory, a registry of sessions, a stop watch and a restart message.
`HostSessionSockets::adopt_inherited` serves sockets handed over by `tddy-supervisor`. `LaunchHost::session_identity`
replaces `session_github_credential`; the split-agent resume path no longer adds pairs.

Detail: [session-identity.md](../session-identity.md).
