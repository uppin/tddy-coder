# 2026-09-24 — github.pending_login_ttl_seconds

**Type:** Feature

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

New `pending_login_ttl.rs`: `PendingLoginTtl` — absent → `DEFAULT_PENDING_LOGIN_TTL_SECONDS` (600), `0` → never, at most `MAX_PENDING_LOGIN_TTL_SECONDS` (604,800); a larger, negative or non-numeric value fails the config load naming the setting. `GitHubConfig.pending_login_ttl_seconds` is the only change to `config.rs` (+2 production lines, 1,472 → 1,474; `oversized-file-config` regressed, deferred with consent). The `auth_storage` doc comment names the credential vault instead of `github-tokens.json`. Detail: [daemon-kernel.md](../daemon-kernel.md#pending_login_ttl--githubpending_login_ttl_seconds).
