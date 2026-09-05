# 2026-07-26 — pr-stack-branch-gated-spawn: `deriveStackBaseBranch` no longer previews `branch ?? branchSuggestion`

**Type:** Fix

only a **created** `branch` names a ref, so a parent holding just a suggestion is passed over like an absent one and the Start-Session dialog can no longer promise a base the daemon's branch-gated spawn then refuses. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
