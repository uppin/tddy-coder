# 2026-08-13 — tddy-daemon — `connection_service.rs` repeats a trim-to-`Option<String>` block six times

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`connection_service.rs` has the same eight-line "trim, empty means unset" block at five pre-existing
sites plus the one this changeset added, and the file already contains exactly that helper nested
inside `resume_agent_and_recipe`. Hoist it to module scope and collapse all six. The new site was left
consistent with its five siblings rather than fixed in isolation.

Related, in the same file: `validate_stack_seed_base_session` and `require_pr_stack_orchestrator` are
pure free functions with no `&self`, and belong in a `connection_service/pr_stack.rs` whenever that
13k-line file is finally split.
