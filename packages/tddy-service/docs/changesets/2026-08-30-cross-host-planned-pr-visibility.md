# 2026-08-30 — cross-host planned-PR visibility

**Type:** Bug Fix + Feature

`StartSessionRequest.stack_node_id = 37` names the planned node a pr-stack child materializes, so the daemon links by identity instead of re-deriving the node from a branch the operator can rename; `ResolveStackBaseRequest.stack_node_id = 6` carries the same id to the peer leg, without which the base-gate hole survived cross-host; new `LinkStackNode` RPC (`{daemon_instance_id, orchestrator_session_id, node_id, child_session_id, branch}` → `{stack_plan_json}`) is the write half of `ResolveStackBase`. All additive; empty `stack_node_id` preserves the existing branch-derived path exactly.
