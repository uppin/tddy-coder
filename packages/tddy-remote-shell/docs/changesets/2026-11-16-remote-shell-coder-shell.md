# 2026-11-16 — The Coder transport: a workspace behind the port

**Type:** Feature · `#remote-shell` 4/7, PR [#918](https://github.com/uppin/tddy-coder/pull/918)
Cross-package entry: [`docs/dev/changesets/2026-11-16-remote-shell-coder-shell.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-coder-shell.md)

`CoderShell` is the port's second transport: `coder ssh --disable-autostart` on
`<owner>/<name>[.<agent>]`, with the deployment URL and session token in that child's
environment alone. It passes `contract::assert_behaves_like_a_remote_shell` against a
stand-in `coder` executable. A workspace that is not running is `Unreachable`, told from a
failed command by a start marker on stderr. See [architecture.md](../architecture.md).
