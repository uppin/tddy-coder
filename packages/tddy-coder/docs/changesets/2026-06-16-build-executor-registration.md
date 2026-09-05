# 2026-06-16 — Build executor registration

**Type:** Feature

`build_executor.rs`: `TddyBuildExecutor` implements `tddy_core::toolcall::BuildExecutor` over `tddy-build`; `run.rs` calls `build_executor::register()` before starting the toolcall listener so relayed `build`/`build-list` run co-located. New dep on `tddy-build`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder, tddy-build)
