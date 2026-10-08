# 2026-10-08 — A Telegram-started session says the project's account is unavailable

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

A Telegram start has no session token, so no vault can be opened: the session starts under the checkout's
identity with no GitHub token, and when the project assigns an account the chat is told, and the same text is
logged at `warn`. Tool sessions are started with no host-session socket, an explicit refusal marked
`TODO(stdio-relay)` in `workflow_spawn.rs`.

Detail: [architecture.md](../architecture.md#github-account-of-a-telegram-started-session).
