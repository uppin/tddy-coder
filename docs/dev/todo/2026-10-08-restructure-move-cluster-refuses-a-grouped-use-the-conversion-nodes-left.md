# 2026-10-08 — the agents cluster move is refused on a grouped `use` of co-moving modules

**Category:** Restructure engine limit, met on lifecycle's import shape
**Source:** #carve 21/21 (PR #536), R6 (`tddy-session-agents`) of
[2026-09-26-carve-lifecycle-moves](../changesets/2026-10-09-carve-lifecycle-moves.md)

## What the engine did

`restructure check --deep` on one `move_cluster_to_crate` anchored on `agent_host_callbacks` with the other
eleven T3 modules in `also` (`svc_provision_agent_clone`, `svc_start_hosted_agent_clone`,
`svc_turn_end_reporter`, `agent_roster`, `seeded_clone_guard`, `seed_codebase`, `roster_replacement`,
`peer_session_answer`, `svc_resolve_listed_worktree`, `session_dir_lookup`,
`svc_ensure_session_room_for_agents`) is refused before any server answers:

```text
Error: the index daemon refused this run (InvalidArgument): plan is malformed: `crate::connection_service::seed_codebase` is one of several paths a single `use` writes, and moving the module gives them different qualifiers — write one `use` per path so each can be re-pointed on its own
```

The `use` is `agent_host_callbacks.rs:19`:
`use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};`
(`SeededAgentClones` is reached through `connection_service`'s `pub use seed_codebase::*`, the two modules are
co-moving members, so the engine gives the group's leaves different qualifiers). `crate_move/header.rs`
`one_use_per_path` is the refusal, by design: "one prefix is all a group has".

## What the node did about it

Nothing. The fix is a source edit to split the group into one `use` per path before the move, which the
changeset's Boundaries do not allow a hand edit to do (hand edits are build corrections after a move) and the
developer's rule is that an engine refusal means stop and ask. The conversion nodes' A4 check was written to
keep exactly this import shape out of the topic modules; it did not reach this file.

## Why deferred

Either the engine learns to split a group whose leaves need different qualifiers (`repoint_facade_imports`
already does, `repoint_facade/group.rs` Rule S), or the developer consents to a pre-move edit of the grouped
`use` lines in the T3 files. Other T3 files may carry the same shape: the check stops at the first, so it is
not known how many.
