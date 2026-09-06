# 2026-08-17 — `restores_a_clone_that_diverged_and_says_so` is suite-order sensitive

**Category:** Known failing test
**Source:** session-agent-roster changeset, 2026-08-17

- `packages/tddy-daemon/tests/session_agent_remote_acceptance.rs` — passes **3/3 in isolation**
  (`-- --test-threads=1 restores_a_clone_that_diverged`, ~6 s each) and passes in most full-suite
  runs, but failed once in a full `--test-threads=1` run against a shared testkit
  (`LIVEKIT_TESTKIT_WS_URL`), where every Docker port flake is already eliminated. So it is neither
  the port flake nor a hard regression — it is timing-sensitive within the suite.
- It asserts the reconcile path: corrupt the clone by hand, move the session, and expect the mirror
  to restore the clone **and record a divergence**. The likely sensitivity is the window between
  `mirror.apply` writing and `restore()`'s divergence check, which
  `session_agent_clone.rs` now gates on `mirror.marker().last_seq == restored_at_seq` (the fix for
  a *false* divergence on every ordinary edit). Under suite load the ordering that makes the
  hand-corruption observable can be missed.
- Worth fixing properly rather than retrying: the test should drive the corruption against a
  signal the fixture controls (hold the mirror, corrupt, release) instead of racing a poll tick.
