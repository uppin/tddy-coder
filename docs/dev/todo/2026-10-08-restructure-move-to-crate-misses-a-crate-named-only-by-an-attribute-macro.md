# 2026-10-08 — the destination's manifest lacks `async-trait` when only `#[async_trait]` names it

**Category:** Manual fix after an engine move (a manifest line the engine missed); external crate, already a dependency of the origin
**Source:** #carve 21/21 (PR #536), R5b: `demo_vm_service` → `tddy-demo-vm-service`. Engine cause: [2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path](2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md)

The moved file has `use async_trait::async_trait;` and `#[async_trait] impl DemoVmService for DemoVmServiceImpl`. The engine added no `async-trait` to `packages/tddy-demo-vm-service/Cargo.toml`
(`E0432: unresolved import async_trait`, then three `E0195` from the unexpanded attribute). **Hand fix:** `async-trait = "0.1"` (the version lifecycle declares) added to `[dependencies]`.
This is the same external crate lifecycle already depends on, not a new one in the dependency tree, but it is not on the developer's list of approved edges for the crate (the seven internal ones), so it is recorded here.
