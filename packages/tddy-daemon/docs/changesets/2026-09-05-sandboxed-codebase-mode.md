# 2026-09-05 — a workspace jail resolves its worktree without listing its neighbours

**Type:** Fix

The worktree ancestors a `workspace` jail needs to canonicalize its own checkout are granted as `metadata` reads rather than `literal` ones. `<tddyhome>/sessions` is one of those ancestors and its entries are the other live sessions on the host, so an ordinary `file-read*` grant let one session's jail enumerate its neighbours by id. Lookup is all the jail needs and now all it has; on Linux the grant maps to no bind mount, replacing a read-only bind of `/` onto `/`. `IN_JAIL_TOOL_TIMEOUT` is imported from `tddy-sandbox-runner` instead of declared here, so the two hosts of the same exchange cannot drift. Feature [sandboxed-codebase-mode.md](../../../../docs/ft/coder/sandboxed-codebase-mode.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
