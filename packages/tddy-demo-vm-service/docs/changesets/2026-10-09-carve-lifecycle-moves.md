# 2026-10-09 — New crate: the demo VM service handlers

**Type:** Architecture

Created by `#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)): `DemoVmState`,
the coordinate handlers and `DemoVmServiceImpl`, 329 production lines. `DemoVmServiceImpl::new` takes the
`DemoVmState` the host builds instead of the host, so the type can live outside lifecycle (an inherent
`impl` cannot leave its type's crate). See [demo-vm-service.md](../demo-vm-service.md).
