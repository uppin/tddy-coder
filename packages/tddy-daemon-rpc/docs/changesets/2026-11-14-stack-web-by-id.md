# 2026-11-14 — Project stacks served by address on their owning host

**Type:** Feature

`ListProjectStacks` and `GetProjectStack` serve a project's stacks from the project stack store,
fanning out one hop to eligible peers and naming unreachable hosts. Every PR-stack request resolves
a `StackAddress` (or a `session_id` through its changeset's `stack_id`) and forwards to the owning
daemon. Handlers live in `src/pr_stack/project_stacks.rs` and `project_stack_queries.rs`;
`ports.rs` delegates. Authentication precedes address validation.
