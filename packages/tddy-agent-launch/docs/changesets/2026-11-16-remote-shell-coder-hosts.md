# 2026-11-16 — A Coder target resolves to a shell in its workspace

**Type:** Feature

`coder_target::shell_for_coder_target` reaches a session's Coder target: the caller's account is read from their vault through `CoderCredentialSource` and a `CoderShell` is built on `<owner>/<workspace>[.<agent>]` through the daemon's `coder` CLI. A target that is not a Coder target, names no workspace, names an account the vault does not hold, or whose vault is locked is refused with that reason (`CoderTargetError`, this PR's own type). Once the shell is resolved, the launch, hooks carrier and follower are the SSH target's path unchanged. At wrap, `CliSessionStart` and `shell_for_session_target` moved from `opencode_cli_spawn.rs` (476 → 529 production lines) into `opencode_cli_spawn::session_start`, leaving it at 422. See [the cross-package entry](../../../docs/dev/changesets/2026-11-16-remote-shell-coder-hosts.md).
