# 2026-08-13 — pr-stack — seeding a stack from several existing sessions

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

- **Only one base session can seed a stack.** The picker is single-select and
  `seed_stack_with_base_session` refuses a second node. Seeding a chain over *several* pre-existing
  branches would declare dependencies their git history does not have: making the chain real means
  rebasing branches an operator may be actively working in, and leaving it unreal means every node
  below the first reports itself behind its base from the moment the stack exists. Neither was worth
  shipping to get an ordering control.
- **With multi-select would come ordering.** The original request asked for drag-handle ordering over
  the selected sessions, with the linear order becoming the `parents` chain. It was dropped because a
  single base node has nothing to order. The panel's persisted `display_order` and
  `move_planned_pr_node` are the reorder primitives to build it on; note they move *rows*, while this
  would have to move `parents`, which is `pr_set_parents`' job.
- **claude-cli / cursor-cli sessions cannot seed a stack.** The orchestrator is a tool session, and
  those forms hold no agent + tool-path + model triple valid for spawning one.
