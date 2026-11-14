# 2026-11-14 — Stack plan and read tools served by the daemon

`#orchestrators 7/11` ([#884](https://github.com/uppin/tddy-coder/pull/884)).

- **The eleven stack plan and read tools are daemon exec tools** addressed by `stack_id`:
  `pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`,
  `pr_comments`, `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`,
  `pr_set_status`, `pr_adopt`. A call naming another host's `daemon_instance_id` is
  forwarded there.
- **Offered by role, never by recipe.** An orchestrator-role session gets all eleven for
  any stack of its project (its changeset's stack is the default); a session bound to a
  node of a stack gets `pr_stack_status` and `pr_read` for that stack alone; every other
  session gets none.
- **Stacks have an optional title**, set by `pr_stack_create` and shown by
  `pr_stack_list`.
- GitHub calls use the **project account's** token. The six git tools (`pr_merge`,
  `pr_repoint`, `pr_close`, `pr_resolve_conflicts`, `pr_set_parents`, `pr_spawn_child`)
  stay in `tddy-tools` until 10/11.

See [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools).
