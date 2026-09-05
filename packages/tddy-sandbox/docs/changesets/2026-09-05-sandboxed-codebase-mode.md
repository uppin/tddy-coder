# 2026-09-05 — a read grant for lookup without listing

**Type:** Feature

`ReadKind::Metadata` and `ReadSpec::metadata(host, reason)` grant what resolving a path through a directory needs — an `lstat` of each component — and nothing more. The other kinds answer "which paths does this rule match"; this one narrows the operation, because `file-read*` on a directory also permits listing its entries, so a grant made only to let a jail canonicalize its own checkout was handing over the names of everything beside it. Backends render it as the narrowing it is: macOS emits a separate `(allow file-read-metadata …)` block, Linux maps it to no bind mount at all. Feature [sandboxed-codebase-mode.md](../../../../docs/ft/coder/sandboxed-codebase-mode.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox)
