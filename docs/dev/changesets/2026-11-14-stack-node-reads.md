# 2026-11-14 — A stack node's session sees its stack read tools in a jail and on another host

**Type:** Feature

`#orchestrators` 13/13, PR [#902](https://github.com/uppin/tddy-coder/pull/902). Feature:
[PR stacking § PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools).

## What changed

- **`tddy-tool-engine`** — new `stack_tools::caller_records`: the record-reading
  classification (`stack_caller_from_records`, moved down from `tddy-daemon-rpc`'s
  `stack_caller_of`, which is now a thin adapter), the combined offer
  (`stack_and_git_tools_offered_to`), and the forwarded-caller pair
  (`stack_bound_to_session_here`, `forwarded_stack_caller_role`).
- **`tddy-session-lifecycle`** — the jail `ListExecTools` bridge
  (`list_exec_tools_from_jail`) classifies the session from
  `BoundJailSession::sessions_base()` and appends the same combined offer the daemon
  appends, so a node session in a jail is listed `pr_stack_status` and `pr_read`.
- **`tddy-daemon-rpc`** — a forwarded stack-tool call is confirmed with the caller's
  owner first, then the role is decided per tool family from the owner's row and this
  host's own store: a node bound here is served the two reads, every other stack tool
  and every stack git tool stays refused to it, and the project-home tools still
  require an orchestrator role.

## Resolved backlog

- `2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md` — deleted. The jail
  bridge lists a node session's two reads, covered by
  `a_stack_node_session_in_a_jail_is_listed_its_two_reads_as_the_daemon_lists_them`
  and `a_stack_node_jail_is_answered_status_and_read_for_its_stack`.
- `2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md` — the
  section "Also: a stack node's read tools on another host" is removed: the host that
  holds the stack now checks its own store for a node bound to the caller, covered by
  `a_stack_node_session_on_another_host_reads_its_stack_where_the_stack_is_held` and
  the stranger-refusal guard. The file's main item, the conversation spawner's
  forwarded start tools, stays open.

## Left as recorded debt

- `2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md` —
  a node session whose orchestrator and stack live on another host is not listed the
  two reads by its own host's `ListExecTools`; that host holds no record of the
  binding. Recorded as a future enhancement, in line with the developer's earlier
  acceptance of cross-host gaps (2026-11-14).

## Code issues

- No file this PR touched reaches 500 production lines (`caller_records.rs` is created
  at 186 production lines; `project_home_remote.rs` goes 304 → 335). No decomposition
  was needed, and no stack-shared file was split.
- The ⚠ DURING records the changeset carried are unchanged by this PR:
  `complexity-daemon-rpc-handler-handle-rpc` (`handle_rpc` is not touched; the change
  stays inside `list_exec_tools_from_jail`) and the `tddy-daemon-rpc`
  `exec_tool/ports.rs` complexity records (`ports.rs` is not edited). No code-issue
  record is claimed by #902, and none was added, closed or worsened.
- `packages/tddy-workflow-recipes/generated/schema-manifest.json` changed only in key
  order (semantically identical), as regenerated during green.
