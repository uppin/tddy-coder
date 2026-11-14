# 2026-11-14 — A daemon-run push goes to the project's remote with the account token

**Type:** Feature

`PushIdentity` carries `remote_name` (the project's remote, default `origin`) and
`account_token`, with a `Debug` that redacts the token. `run_git_op`'s push goes to the named
remote, and when a token is present hands git a per-command credential helper
(`-c credential.helper=…` plus the `GIT_CONFIG_*` environment) that echoes the token from
`TDDY_PUSH_ACCOUNT_TOKEN` — the token is never in argv or logs, and a `git credential fill`
from a push hook answers the same way. This closes
`2026-10-23-project-home-git-push-remote-and-token.md` for project-home `PushSessionBranch`
too.
