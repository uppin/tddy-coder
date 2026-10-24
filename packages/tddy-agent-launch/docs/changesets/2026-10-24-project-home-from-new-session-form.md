# 2026-10-24 — EnsureProjectHomeSession sets main up from a start spec
**Type:** Fix

`svc_ensure_project_home.rs` reaches the main the project records (`home_session_id`) when the
request has no `start`: `NOT_FOUND` when none is recorded, the record cleared and `NOT_FOUND` when its
session directory is gone, `resumed` / `running` otherwise. With a `start` spec it sets main up or
replaces it: the spec is checked (`project_home_start_spec.rs`: a requested role, a placement, and a
type other than `claude-cli` after the harness profile is applied are refused), placed on the home
worktree (`on_home_worktree`), checked as the start path would (`refuse_a_claude_cli_start_up_front`
in `cli_branch_starts.rs`), then a served recorded main is stopped, the new one started through
`start_session_with_role` and recorded (`started` / `replaced`). The lock is taken before the
project is read. `home_agent_profile` and the role scan `find_home_session` are removed.
`start_session_core` applies the harness profile itself; `start_session_with_role` takes a resolved
request, and `apply_harness_profile` and `managed_recipe_for` are `pub(crate)`.
`refuse_a_cli_start_without_a_model` (`start_request_checks.rs`) is shared by the start path and the
up-front check. See [module layout](../module-layout.md) and
[project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session).

Code issues, re-measured with `/analyze-code-issues` at `4348f160a` against `c1df8e142`: no finding
is attributable to this change. `complexity-svc-start-session-core-start-session-core`
(`start_session_on_request`) improved from **393 to 384** lines (complexity 49, nesting 6, unchanged);
every other record re-measured here is unchanged. No record closed; none deleted.

Deferred: `docs/dev/todo/2026-10-24-sandboxed-main-needs-session-role.md` (code `TODO` in
`project_home_start_spec.rs`), `docs/dev/todo/2026-10-24-harness-profile-empty-account-not-vendor.md`;
the `stop_previous_main` `TODO` is covered by `docs/dev/todo/2026-10-23-project-home-running-heuristic.md`.
