# 2026-10-08 — `DemoVmServiceImpl::new` takes the host, so T11's service adapter cannot move to `tddy-demo-runner`

**Category:** Deferred from `#carve` 16a (T11 conversion), found by #carve 21/21 (PR #536) R5 preflight
**Source:** [2026-09-26-carve-lifecycle-moves](../1-WIP/2026-09-26-carve-lifecycle-moves.md), "What moves, by engine operation" T11 row

## What the tree has

`packages/tddy-session-lifecycle/src/connection_service/svc_demo_vm_ports.rs` holds, in one file:

- `DemoVmServiceImpl { state: activity_hub::DemoVmState }`, its `impl DemoVmService` (state-only), and
- `impl DemoVmServiceImpl { pub fn new(host: Arc<DaemonSessionHost>) -> Self { … host.demo_vm_service_state() … } }`, and
- `impl DaemonSessionHost { pub fn demo_vm_entry(self: &Arc<Self>) -> ServiceEntry }`.

`activity_hub` and `demo_vm_coordinate_handlers` (the other two T11 modules) are host-free, and a
`move_cluster_to_crate` of just those two passes `check --deep` and `apply --dry-run` (`resolved 1 of 1 operations`).

## What the changeset assumed

That `DemoVmServiceImpl` could be carried to `tddy-demo-runner` too. It cannot as it stands: its inherent `new` names
`DaemonSessionHost`, so after the move it would be an inherent `impl` of a foreign type's constructor naming a type
the destination must not depend on (`E0116` or a cycle). The conversion node (16a) was to leave no topic item naming
the host; this one does.

## What this node did

Nothing yet (R1 stopped the run before R5). The node's rule is that a topic item still naming the host is a parent
defect, to be reported and not converted here.

## Fix, for whichever node owns it

Make the constructor take the state — `DemoVmServiceImpl::new(state: DemoVmState)` — and have the wiring call
`DemoVmServiceImpl::new(host.demo_vm_service_state())` from `demo_vm_entry` (which stays in lifecycle). Then
`DemoVmServiceImpl` and its `impl DemoVmService` are a host-free module and move with the cluster. Note also that
`demo_vm_coordinate_handlers` names `tddy_session_activity::user_sessions_path` and `tddy_workflow_recipes::writer`,
so `tddy-demo-runner` gains `tddy-session-activity` — an edge the changeset's D12 list (kernel, core, rpc, service)
does not name — and `tddy-vm`, `tddy-workflow-recipes` it already has. That edge needs the developer's approval
before R5 applies.
