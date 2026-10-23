# 2026-10-22 — Sandboxed starts on a provider route; jailed OpenCode and Muse

**Type:** Feature

- The sandboxed claude-cli start resolves its route (`sandboxed_start_route`, the person's default
  included) before anything is created, records it, and passes it into the jail
  (`JailLaunch.route`). The sandboxed relaunch re-resolves it (`resume_route`).
  `refuse_route_in_sandbox` is gone.
- `opencode-cli` / `muse-cli` with `sandbox = true` start in a jail
  (`svc_start_sandboxed_cli_harness_session`) and resume by relaunching it
  (`svc_resume_sandboxed_cli_harness_session`). The pure parts live in `cli_harness_jail.rs`: the
  agent kind, the runner model, `require_provider_route` (no account is `FAILED_PRECONDITION`),
  `refuse_first_prompt`, binary resolution and the runner argv. `canonical_exec_path` replaces the
  two inline copies in the claude start and the relaunch.
- `refresh_relaunch_context_dir` takes the session type, so the context-file allow-list follows the
  harness.

Code issues: **added and not closed** `complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session`
(277 → 250 lines; the engine refused the remaining extractions). `svc_start_sandboxed_claude_cli_session.rs`
stays at 493 production lines (the route plumbing was offset by the shared `canonical_exec_path`).

PR [#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
