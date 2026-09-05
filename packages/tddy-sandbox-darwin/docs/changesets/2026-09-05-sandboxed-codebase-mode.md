# 2026-09-05 — file-read-metadata renders as its own block

**Type:** Feature

`render_plan` splits reads by what they grant rather than listing them together: `ReadKind::Metadata` entries leave the `(allow file-read*)` block and land in their own `(allow file-read-metadata …)`, emitted only when at least one grant asked for it — an empty body would be a blanket metadata allow over the whole filesystem. Placement after the read block is safe and stated as such: both are allows, and an allow never revokes an earlier one. `render_read_rule` renders the path *filter*, which `Literal` and `Metadata` share; the operation is the enclosing block's business. Feature [sandboxed-codebase-mode.md](../../../../docs/ft/coder/sandboxed-codebase-mode.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-darwin)
