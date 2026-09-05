# 2026-06-16 — Build extension point

**Type:** Feature

`toolcall/build.rs`: `BuildExecutor` trait + process-global registry (`register_build_executor`/`build_executor`) + `BuildListQuery`/`BuildOptions`; `toolcall/mod.rs`: `BuildListRequestWire`/`BuildRequestWire`, `ToolCallResponse::BuildJson`; `toolcall/listener.rs`: `build`/`build-list` handlers (returns "build support not enabled" when unregistered). No `tddy-build` dependency. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
