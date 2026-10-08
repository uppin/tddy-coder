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
