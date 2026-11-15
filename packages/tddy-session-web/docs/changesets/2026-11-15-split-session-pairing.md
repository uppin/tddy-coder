# 2026-11-15 — A split session's drawer row shows its branch

**Type:** Fix

`SessionDrawerItem` renders `sessions-drawer-item-branch-<id>` beside the
codebase-host badge when a split row carries a recorded branch. A split row
without one, and every co-located row, renders no branch slot.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-pairing.md](../../../../docs/dev/changesets/2026-11-15-split-session-pairing.md).
