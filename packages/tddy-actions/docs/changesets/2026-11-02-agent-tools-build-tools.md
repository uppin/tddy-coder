# 2026-11-02 — A tool binary can be a build target's output

**Type:** Feature

`ToolBinary.target: Option<TargetBinary { target, path }>` and `ToolHost::target_output` (default `None`), resolved first in `resolve_closure`. Cross-package: [agent-tools-build-tools](../../../../docs/dev/changesets/2026-11-02-agent-tools-build-tools.md).
