# 2026-08-13 — pr-stack — a managed claude-cli/cursor-cli session may record a goal id as its state

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

Surfaced while implementing stack seeding; **not** fixed there, because fixing it changes how existing
on-disk sessions resume and that deserves its own review.

`PrStackRecipe::start_goal()` returns the goal id `"analyze-stack"`, and the claude-cli / cursor-cli
spawn paths in `tddy-daemon`'s `connection_service.rs` seed a managed session's position with
`update_state(&mut cs, WorkflowState::new(recipe.start_goal().as_str()))`
(`spawn_claude_cli_session_inner`, `start_sandboxed_claude_cli_session`,
`start_sandboxed_cursor_cli_session`). So `"analyze-stack"` can be persisted as a *state*, while
`PrStackRecipe::next_goal_for_state` matches only the `"AnalyzeStack"` / `"WriteStackPlan"` spellings —
the goal-id spellings fall into the `_ => orchestrate` catch-all, which would read a session that has
done nothing yet as mid-flight and skip planning.

Tool sessions are unaffected, which is why the orchestrator this changeset creates is fine: a tool
session's `changeset.yaml` is written by its own `tddy-coder` process via `ensure_changeset_recipe`,
leaving `Changeset::default()`'s `Init` — and `Init` is in the table.

Before fixing: confirm a managed **claude-cli** session can actually carry the `pr-stack` recipe in
practice (the web only offers the recipe select for tool sessions, but `managed_codebase` claude-cli
spawns do send `recipe`). If it can, the fix is to accept both spellings per state — and it must be
weighed against sessions already on disk in that state, which resume into `orchestrate` today.
