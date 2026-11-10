# 2026-11-03 — Cursor CLI from cursor.yaml

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- The jail bridge (`DaemonRpcHandler`) serves `ReportSessionStatus` and `ReportAgentActivity`, refusing any session but the jail's own (`PermissionDenied`).
