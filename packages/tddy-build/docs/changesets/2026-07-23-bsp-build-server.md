# 2026-07-23 — **bsp-build-server

**Type:** Feature

declarable target metadata + build modes** — `manifest.rs`: `BuildTarget` gains `tags`/`languages` (`Vec<String>`) + `capabilities: Option<TargetCapabilities>` (`compile`/`test`/`run`/`debug`, all `#[serde(default)]` so omitting still parses). New `capabilities.rs`: `enum BuildMode { Compile, Test, Run }` + `resolve_target_metadata(&BuildTarget)` (author-declared wins, else derived from `config.type`). `BuildMode` threaded through `BuildPlugin::lower` (`plugin.rs`), `lower_target` (`lower.rs`), `execute_target`/`ExecuteOptions` (`executor.rs`, rejecting a capability-forbidden mode), and `build_json` (`service.rs`); `graph` carries the mode. Tests: capabilities 4 + rust modes 2. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [bsp-build-server.md](../../../../docs/ft/coder/bsp-build-server.md). (tddy-build)
