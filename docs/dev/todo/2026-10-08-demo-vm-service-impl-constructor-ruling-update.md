# 2026-10-08 — note: the `DemoVmServiceImpl::new(host)` constructor is fixed inside #536, and T11 goes to `tddy-demo-vm-service`

**Category:** Update to [2026-10-08-demo-vm-service-impl-constructor-still-names-the-host](2026-10-08-demo-vm-service-impl-constructor-still-names-the-host.md) (written, not rewritten, because that file said "nothing yet" and named `tddy-demo-runner`)
**Source:** #carve 21/21 (PR #536), developer rulings of 2026-10-08

- **Receiver:** T11 goes to a **new crate `tddy-demo-vm-service`**, not `tddy-demo-runner` (which keeps the QEMU orchestration, untouched). The `tddy-demo-runner` → `tddy-session-activity` edge in the older note therefore does not arise;
  the new crate's approved edges are `tddy-daemon-kernel`, `tddy-core`, `tddy-rpc`, `tddy-service`, `tddy-session-activity`, `tddy-vm`, `tddy-workflow-recipes`, and lifecycle depends on it.
- **R5a (done):** `activity_hub` and `demo_vm_coordinate_handlers` moved there with the engine; `DemoVmServiceImpl` and `demo_vm_entry` stay in lifecycle for now and name `DemoVmState` through the facade.
- **R5b:** the constructor becomes `DemoVmServiceImpl::new(state: DemoVmState)` (the developer's approved signature exception), the wiring calls `DemoVmServiceImpl::new(host.demo_vm_service_state())` from `demo_vm_entry`, then `DemoVmServiceImpl` moves too.
  Done in this PR, after the host-block moves; that older file is deleted at wrap together with this note.
