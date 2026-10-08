# 2026-10-08 — grouped `use` lines split by hand, and `SeededAgentClones` named through its defining module, before the agents cluster move

**Category:** Manual fix after an engine refusal (hand workaround, developer-consented 2026-10-08)
**Source:** #carve 21/21 (PR #536), R6 preparation. Engine causes:
[2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left](2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left.md)

## Each hand edit (commit "one use per path and a pub peer_session_answer …", before any engine op)

| File and line | Before | After |
|---|---|---|
| `connection_service/agent_host_callbacks.rs:19` | `use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};` | `use crate::connection_service::seed_codebase;`, `use …::seeded_clone_guard;`, `use crate::connection_service::seed_codebase::SeededAgentClones;` |
| `connection_service/svc_ensure_session_room_for_agents.rs:1` | `use crate::connection_service::{agent_roster, seed_codebase, seeded_clone_guard};` | three one-path `use`s |
| `connection_service/svc_start_hosted_agent_clone.rs:7` | `use crate::connection_service::{agent_roster, peer_session_answer};` | two one-path `use`s |

## A second engine problem, found after the split

With the grouped `use` split, `check --deep` still refused: `agent_host_callbacks.rs … names
tddy_session_lifecycle::connection_service::seed_codebase::SeededAgentClones, which stays behind`, although `seed_codebase`
was a member of the same `move_cluster_to_crate`. The path was written `crate::connection_service::SeededAgentClones`, i.e.
through `connection_service`'s `pub use seed_codebase::*;`. Naming the trait through its defining module
(`…::seed_codebase::SeededAgentClones`, line 21) made the check pass. So the engine, following a glob facade to the defining module,
does not treat that module as co-moving. (Engine fix: resolve the facade and then test membership in the moving set, as it does for
a path written through the module.)

## What the engine should do so this is automatic

Split a grouped `use` whose leaves need different qualifiers (`repoint_facade_imports` Rule S already does), and treat a path
that reaches a co-moving member through a glob facade as co-moving. Delete this file with that fix.

## A third hand edit, to avoid an unapproved edge (R6)

`connection_service/agent_host_callbacks.rs:30`: `use tddy_sandbox_runner::ExecuteToolResponse;` → `use tddy_service::proto::exec_tools::ExecuteToolResponse;`.
`tddy_sandbox_runner` re-exports that very type (`tddy-sandbox-runner/src/lib.rs:28`), so the trait signature is unchanged. Without this the engine wrote
`tddy-session-agents → tddy-sandbox-runner` into the manifest, an edge the developer did not approve (D7-A). The engine should prefer the defining crate
of a re-exported name when it chooses which crate the destination gains (it already does in the path survey for `use` of facades of the same crate).

## R8 (`tddy-session-split`): hand edits before the cluster move

All in `packages/tddy-session-lifecycle/src/`; imports and visibility only, each because `check --deep` refused the plan with "still names
`tddy-session-lifecycle`" / "stays behind" until it was made:

| File | Edit | Why the engine refused |
|---|---|---|
| `connection_service/svc_start_sandboxed_codebase_session.rs:18` | `use super::{agent_roster, AttachmentProgressSink};` split into two `use`s | grouped `use` (the first filed engine limit) |
| `…/svc_spawn_split_agent.rs:15`, `…/svc_start_sandboxed_codebase_session.rs:19`, `…/split_start/split_claude_cli_start.rs:14` | `use super::AttachmentProgressSink` (and `super::super::`) → `use tddy_session_files::attachment_progress::AttachmentProgressSink;` | a glob facade of the origin: the engine reads the facade's module as staying behind |
| `…/svc_spawn_split_agent/svc_paired_codebase_teardown.rs:15`, `…/split_start/split_claude_cli_start.rs:1` | `super::super::SplitStartFailure` → `crate::connection_service::split_start::SplitStartFailure` | same: named through the `pub use split_start::*` facade |
| `…/attached_initial_prompt.rs` | `use super::svc_materialize_staged_attachment::AttachmentState` → `use tddy_session_files::svc_materialize_staged_attachment::AttachmentState`; `pub(in crate::connection_service) async fn` → `pub async fn` | a `pub(in crate::connection_service)` path counts as naming the origin crate (and could not resolve after the move anyway) |
| `workspace_session.rs:74,75,140,198,199,244` | `crate::connection_service::{find_registered_project, project_repo_root, starting_session_metadata}` → `crate::connection_service::service_util::…` | names through the origin's facade; the cluster member `service_util` was read as staying behind |
| `connection_service.rs` | `mod service_util;` → `pub mod service_util;` | needed so the line above compiles before the move |

Also: `hooks_and_urls` was **left out of the split cluster**. The changeset assigns it to T4, but every user of it is a launch (T1) module, and its
`StartingClaudeCliSession` names `stack_parent::SpawnStackParent` (T9), so moving it into split would make split name launch. It moves with R9.

What the engine should do: follow a facade to the defining module and test that module for membership in the moving set, and not treat
`pub(in crate::origin_module)` as naming the origin crate. Delete this section with that fix.

## R9 (`tddy-agent-launch`): hand edits before the cluster move (commit "imports and visibility the engine needs before the launch cluster move")

Imports, paths and visibility only, over the 46 modules of the cluster; every one made because `check --deep` refused the plan until it was made, and all were applied by small scripts (kept in the author's scratch, not in the repo) and then compiled:

1. **Grouped `use` lines split to one path each**: 14 flat groups and one nested group (`use crate::{cli_session_manager::…, connection_service::{…}}`) in 15 files.
2. **A path through the origin's glob facade to a member or a moved module, re-spelled through its defining module or crate** (the same engine blind spot as R6/R8): `AttachmentProgressSink` → `tddy_session_files::attachment_progress::…`;
   `SeedCodebase`, `SeededAgentClones`, `roster_replacement_pairs`, `agent_host_callbacks`, `agent_roster`, `seed_codebase`, `peer_session_answer`, `SeededAgent`, `started_roster_rev` → `tddy_session_agents::…`;
   `service_util`, `split_ports`, `attached_initial_prompt`, `resolve_split_agent_placement`, `create_session_worktree`, `find_registered_project`, `index_session_worktree`, `project_repo_root`, `push_new_branch_to_origin_if_requested`,
   `starting_session_metadata`, `write_initial_changeset` → `tddy_session_split::…`; `CodebasePlacement`, `classify_placement`, `PlacementRequest`, `resolve_os_user` → `tddy_daemon_livekit::…`; `daemon_hook_urls` → `tddy_daemon_kernel::…`;
   `svc_materialize_staged_attachment`, `AttachmentMaterialization` → `tddy_session_files::…`; and cluster members named through the facade (`recipe_enables_conversation_spawn`, `WorktreeSource`, `StackParentHost`, `SpawnStackParent`, `ManagedLaunch`,
   `effective_spawn_branch`, …) → `crate::connection_service::<member>::…`.
3. **Visibility so those paths resolve before the move**: `mod worktree_source;`, `mod hooks_and_urls;`, `mod stack_parent;` → `pub mod`; `pub(crate) mod host_session_socket;` and `pub(crate) mod session_acting_identity;` → `pub mod`;
   `pub(in crate::connection_service)` → `pub` (21 occurrences in `svc_start_session_core.rs`, `tool_session_spawn.rs`, `tool_spawn_plan.rs`): the engine counts a `pub(in crate::origin_module)` path as naming the origin crate.
4. **Plan correction**: `family_proto_bridge` (14 lines, host-free, named by `session_coordinate_handlers`) joins the cluster; the changeset lists it under the staying wiring, but a launch module cannot name lifecycle. Its facade `pub use family_proto_bridge::wire_same;` stays in lifecycle.
