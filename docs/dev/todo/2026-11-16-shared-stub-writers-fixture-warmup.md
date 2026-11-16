# 2026-11-16 — The shared stub writers are not warmed before a timed launch

**Category:** Future enhancement — deferred from `#remote-shell` 6/7 ([#920](https://github.com/uppin/tddy-coder/pull/920))
**Source:** `packages/tddy-session-lifecycle/tests/agent_registry_support/mod.rs` (`a_stub_agent_script_minting`, installed as `acme`), `packages/tddy-session-lifecycle/tests/harness_clis_support/mod.rs` (`a_stub_harness_script`, installed as the harness binary)

## What happens

A freshly written executable's first exec takes ~2.4 s on macOS (measured during #920, against
~20 ms afterwards) — a first-execution scan the writer pays inside whatever window the test
then measures. #920 warmed every `write_executable` in the suites it timed
(`TDDY_FIXTURE_WARMUP`: the script carries a guard as its first line and is run once, guarded,
so the scan is paid before any launch is timed; the guard records no call and runs nothing).

The two shared stub writers above are **not** warmed. Their stubs are written and then spawned
by the daemon under test, and their reports are awaited under a 90 s `TERMINAL_OBSERVE` bound,
so the scan has not been seen to flake a test — but any future test that times a launch of one
of these stubs (a startup window, a deadline, a latency assertion) will spend the scan inside
its window on macOS.

## Why deferred

No test of #920 times an executable these writers write: #920's lifecycle suite uses only
`builtin_yaml_as` from `agent_registry_support` and times its own warmed stub. Warming the
shared writers changes fixtures other PRs' suites depend on, so it is its own change, not a
rider on #920.

## What would close it

Give both writers the same warm-up #920's timed suites use: the written script carries
`if [ -n "$TDDY_FIXTURE_WARMUP" ]; then exit 0; fi` as its first line after the shebang, and the
writer runs it once with that variable set, asserting success, before returning. The guard must
record no call, touch no marker and write no pid file. Pin it with the suites that already use
these writers (`./test -p tddy-session-lifecycle` scoped to them) staying green.
