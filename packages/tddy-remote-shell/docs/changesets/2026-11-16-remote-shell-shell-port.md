# 2026-11-16 — The crate is created: one remote-shell port, SSH as its first transport

**Type:** Feature · `#remote-shell` 1/7, PR [#915](https://github.com/uppin/tddy-coder/pull/915)
Cross-package entry: [`docs/dev/changesets/2026-11-16-remote-shell-shell-port.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-shell-port.md)

New crate `tddy-remote-shell`: the `RemoteShell` trait (one-shot `exec` with stdin and a
deadline, `exec_streamed`, and `interactive` as an argv for a local PTY), `ShellAddress`,
`ExecOutput`, `FileMode`, and the four-variant `RemoteShellError`. `SshShell` is the OpenSSH
transport and the only place the workspace spawns `ssh` for a remote command; every call
carries `BatchMode=yes`, `ConnectTimeout=15`, `ServerAliveInterval=5`,
`ServerAliveCountMax=3`. Behind `test-support`: `InMemoryShell` and the
`contract::assert_behaves_like_a_remote_shell` suite, run against both the double and a
stand-in `ssh` executable. See [architecture.md](../architecture.md).
