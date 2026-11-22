# 2026-11-21 — The `ToolHost` test fixture, once

**Type:** Refactor · **Stack:** `#tab-hardening` 2/4 (PR #990)

The crate is new: `a_tool_host()` replaced `AHostOfStubs`, `AHostOfBinaries`, `AStubPathHost`, `AHostWithoutAnyAgent` and every
`AFixtureHost` / `AHostOf` in `tddy-session-lifecycle` and `tddy-agent-launch`, with their assertions unchanged.

Full record: [`docs/dev/changesets/2026-11-21-tab-hygiene.md`](../../../../docs/dev/changesets/2026-11-21-tab-hygiene.md).
