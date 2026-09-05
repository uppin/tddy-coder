# 2026-07-25 — pr-stack-live-status: `Stack::base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>`

**Type:** Feature

the single source of truth for a planned node's spawn base. Returns the nearest non-merged ancestor's `origin/<branch>` (first `effective_base_refs` entry) or the stack default, after an ordering guard that refuses (`ChangesetInvalid`, naming the parent) when a non-merged parent has not been started (no `session_id`). Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
