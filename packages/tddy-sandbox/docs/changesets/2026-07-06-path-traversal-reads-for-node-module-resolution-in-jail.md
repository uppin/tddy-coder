# 2026-07-06 — `path_traversal_reads` for Node module resolution in jail

**Type:** Feature

`exec_reads::path_traversal_reads` + `cursor_agent_prerequisite_reads` extension so sandboxed `agent` can `lstat('/Users')` during module resolution. Feature [cursor-cli-session.md](../../../../docs/ft/daemon/cursor-cli-session.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#287](https://github.com/uppin/tddy-coder/pull/287). (tddy-sandbox, tddy-sandbox-recipes)
