# 2026-10-22 — Muse Code terminal sessions through the CLI harness registry

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)), across
`tddy-agent-backend` (the `muse-cli` descriptor in `cli_harness_muse.rs`, its curated models, the Muse
arm of `resolve_route` — Meta only, the key in `META_API_KEY` — and the `muse` context-glob row),
`tddy-agent-launch` (start and resume dispatched by a registry lookup, so opencode-cli and muse-cli
share `start_cli_harness` / `resume_cli_harness`; `launch_model_for`), `tddy-worktree-service`
(`BranchIntentPolicy::cli_harness(session_type)`), `tddy-session-activity` (muse-cli worktrees removed
on delete) and `tddy-web` (the Muse Code toggle, one request branch for opencode-cli and muse-cli, and
Muse Code back in the Default-for control). The registry-driven dispatch and its muse start/resume
acceptance tests were an orchestrator decision taken at green, without the per-node acceptance-test
review. New backlog entries: `2026-10-22-muse-code-cli-facts-unconfirmed` (argv, key variable and
`.muse/` tree not checked against a real `muse`) and `2026-10-22-muse-code-headless-backend`. Feature:
[Muse Code CLI session](../../ft/daemon/muse-cli-session.md).
