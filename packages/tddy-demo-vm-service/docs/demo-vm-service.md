# `tddy-demo-vm-service`

The `demo_vm.DemoVmService` handlers: per-session demo VM start, stop and status. About 330 production
lines in three modules; the longest function, `start_demo_vm_at_coordinate`, is 132 lines.

| Module | Holds |
|---|---|
| `activity_hub.rs` | `DemoVmState` and `DemoVmHandle`: the per-session VM table (shared with the host), `tddy_data_dir`, `user_resolver`, `rpc_activity` (shared) and `config` (a clone) |
| `demo_vm_coordinate_handlers.rs` | `impl DemoVmState`: start, stop and status at a coordinate |
| `demo_vm_service.rs` | `DemoVmServiceImpl`, the thin `DemoVmService` adapter over a `DemoVmState`; `DemoVmServiceImpl::new(state)` |

The daemon host builds the state (`DaemonSessionHost::demo_vm_service_state()` in
`tddy-session-lifecycle`) and hands it to the constructor; the constructor takes the state, not the host, so
the type lives in this crate. It depends on `tddy-daemon-kernel`, `tddy-session-activity`, `tddy-vm` and
`tddy-workflow-recipes` and on none of the session crates. `tddy_session_lifecycle` re-exports the three
modules.
