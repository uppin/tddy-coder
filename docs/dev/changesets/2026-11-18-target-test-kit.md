# 2026-11-18 — Target-session tests share one stand-in kit, and a live test runs the Coder path

**Type:** Refactor (tests) · `#coder-start` 9/10, PR [#949](https://github.com/uppin/tddy-coder/pull/949)

- `tddy_testing_commons::remote_target` holds `write_executable` (replacing copies in 11 test files)
  and `StandInSshTarget` (replacing four identical lifecycle copies). Suites unchanged by name and
  count; assertions unchanged.
- `tddy-session-driver` gains an optional `--session-id` (default: a fresh UUID), so a caller can lay
  out `<clone>/.worktrees/<id>` and get Claude's clone pre-trust — the one production change, approved
  as a scoped exception.
- `tddy-session-driver/tests/coder_live_production.rs`: production test against a real Coder
  deployment. All three pass on `mantasi/remote-dev` (192 s): `coder version` reads `Coder v2.33.5…`,
  the workspace reports `running`, Cursor reaches `Cursor Agent`, Claude reaches `❯ Try` with no trust
  dialog. Claude's prompt marker was corrected from the guessed `for shortcuts`.
- Backlog narrowed, not closed: *2026-11-16 — stand-in target, stub agents and daemon helpers
  repeated* (diverged copies remain) and *2026-11-16 — Coder hosts never run against a real
  deployment* (hook follower and a stopped workspace still unverified).
- Downgrading both ✅ RESOLVED HERE entries to narrowed: orchestrator, under the user's finish-without-stopping instruction (their open parts were not covered).
- Validation (2026-11-18): scoped build of `tddy-session-driver`/`tddy-testing-commons` clean; production files under the 500-line budget (`main.rs` 185, `remote_target.rs` 124); no criticals.
