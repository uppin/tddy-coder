# 2026-11-02 — Tool targets are ToolDefs; hermetic actions

**Type:** Feature

`builtin::ToolTarget::{Inline, Ref}` (legacy shape refused), `graph` orders a tool's producing target first, `executor::tool_run` resolves `tool_dep_ids` through the tool registry, `BuildTools`/`execute_target_with_tools`/`ExecuteOptions.hermetic`, `BuildAction.sandbox`, the `ActionSandbox` port, `cache::compute_run_cache_key` (tool fingerprints + hermeticity). Example pipeline migrated. Cross-package: [agent-tools-build-tools](../../../../docs/dev/changesets/2026-11-02-agent-tools-build-tools.md).
