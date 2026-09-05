# 2026-06-29 — Unified action execution runtime

**Type:** Feature

new leaf crate: `ActionSpec`/`ActionKind`, `ProcessRuntime` (subprocess + Combined/StdoutStderr), `PtyRuntime` (portable-pty), `PipelineRuntime` (mapper/primary/transform), `ActionCatalog`, manifest/`BuildAction` converters, `result_kind` post-processors. Consumed by `tddy-daemon`, `tddy-core` session actions, `tddy-build`. Feature [background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). PR [#244](https://github.com/uppin/tddy-coder/pull/244). (tddy-actions, tddy-task, tddy-daemon, tddy-core, tddy-build)
