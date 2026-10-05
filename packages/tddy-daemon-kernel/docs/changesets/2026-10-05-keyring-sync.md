# 2026-10-05 — `keyring.group_secret`: which peers may hold this daemon's credentials

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

`DaemonConfig.keyring: Option<KeyringConfig>`. Absent means this daemon propagates credentials to
nobody, not to everyone in the common room. Detail:
[daemon-kernel.md](../daemon-kernel.md#keyringgroup_secret--which-peers-may-hold-this-daemons-credentials).
