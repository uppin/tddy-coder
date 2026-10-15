# 2026-10-15 — Local gh: child sessions ignore the parent's source; Telegram cannot override `gh`

**Category:** Deferred scope (developer consent at `/pr-wrap`, 2026-10-15)
**Source:** [2026-10-14-local-gh-github-source.md](../1-WIP/2026-10-14-local-gh-github-source.md)

## 1. Child sessions read this host's row, not the parent's forwarded source
Conversations and stack children spawned from a session use `session_account_access(token)`, which carries
no source, and their session dirs carry no `github-source.yaml`. A parent started with a forwarded
`github_source` therefore has children that re-read this host's `projects.yaml` row.
**Would close it:** copy the parent's sidecar into the child's session dir at spawn.

## 2. Telegram builds its own `GithubCli`
`workflow_spawn::telegram_git_environment` uses `GithubCli::on_spawn_child_path()`, so
`DaemonSessionHost::with_github_cli` never reaches it. Threading one in changes the
`TelegramWorkflowSpawn` struct literals (three acceptance tests and `runtime.rs`).
**Would close it:** a `github_cli` field on `TelegramWorkflowSpawn`, set from the host.

## 3. Unidentified flake in `ProjectLocalGh.cy.tsx`
One combined Cypress run showed 5 of 6 passing; the failing test was not captured. Four later runs and
the post-split run were 6 of 6. Capture the failing test name if CI shows it.

## 4. Blocking `gh api user` on async start paths
Bounded by the probe timeout, same as the daemon's existing identity resolution; Telegram's
`resolve_spawn_inputs` now does it too. Move to `spawn_blocking` if start latency shows it.
