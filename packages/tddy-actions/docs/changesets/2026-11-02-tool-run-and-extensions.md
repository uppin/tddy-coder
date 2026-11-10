# 2026-11-02 — `ToolDef` gains `run:` and owner-validated `extensions:`

**Type:** Feature

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)).
- New `tool/run.rs` (`RunSection`, `Transport`, `ArgvItem::{Lit, When, Splice}`) and `tool/template.rs` (`TemplateParams`, `render_argv`).
- New `tool/extensions.rs` (`ExtensionOwner`).
- New registry entry points `ToolRegistry::from_embedded` and `load_with_owners`, and `BUILTIN_SUPPORT_TOOLS` holding `tools/tddy-tools.yaml`.
- New `ToolError` variants `UnknownExtension`, `ExtensionRejected` and `UnboundPlaceholder`.

See [declared-tools.md](../declared-tools.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
