# 2026-11-14 — A stack node's session sees its stack read tools in a jail and on another host

`#orchestrators 13/13` ([#902](https://github.com/uppin/tddy-coder/pull/902)).

- A session bound to a node of a PR stack is listed `pr_stack_status` and `pr_read` by
  the jail's `ListExecTools` bridge, exactly as the daemon lists them.
- The host that holds a stack serves those two reads to a node session held on another
  host, checking its own store for the node bound to the caller. Stack writes and the
  stack git tools stay refused to a node caller.
- A node session whose stack lives on another host is still not listed the reads by
  its own host, which holds no record of the binding.

See [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools).
