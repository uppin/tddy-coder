# 2026-07-11 — Wire `TDDY_TOOLS_ACCOUNTING_FILE`

**Type:** Feature

both the claude and cursor `tddy-tools --mcp` spawn sites set `TDDY_TOOLS_ACCOUNTING_FILE = <egress-dir>/accounting.json` (mirrors `TDDY_TOOLS_LOG_FILE`), so the host-visible session egress dir carries the subagent token accounting for `tddy-sandbox-app` to read. Feature [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#289](https://github.com/uppin/tddy-coder/pull/289). (tddy-sandbox-runner)
