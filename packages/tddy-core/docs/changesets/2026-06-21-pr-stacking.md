# 2026-06-21 — PR stacking

**Type:** Feature

`Stack`/`StackNode` structs + `stack`/`orchestrator_session_id` optional fields on `Changeset`; atomic write helpers `update_stack_atomic`, `link_stack_node_to_child_session`, `sync_stack_node_from_child`; `topo_order`/`effective_base_refs`/`node` on `Stack`; `is_skipped` on `StackNode`; transport-agnostic `spawn_chain_child_worktree` in `session_chain.rs`.
