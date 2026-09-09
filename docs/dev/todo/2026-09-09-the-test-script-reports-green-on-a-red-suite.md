# 2026-09-09 — `./test` reports success on a failing suite, and skips binaries its tests need

**Category:** Known failing test
**Source:** `analyze-coverage-export-and-harness-selection` (#466), `tddy-daemon` baseline

Two independent harness defects that compound: together they let a **38-test-red** daemon suite
report as green. Found while establishing a baseline before analysis, not by looking for them.

- **The exit code is `tail`'s, not cargo's.** `./test` pipes cargo through `tail` to write
  `.verify-result.txt`, so the script exits 0 whatever cargo did. An observed run ended
  `test result: FAILED. 12 passed; 1 failed` and still reported `[exited with code 0]`. Anything
  trusting that status — an agent, a hook, a CI step — reads a red suite as passing. The fix is
  `PIPESTATUS`/`pipefail`, but note `./verify` shares the pattern and should be checked with it.
- **The daemon suite needs sibling binaries `./test` never builds.** `./test` builds `tddy-coder`
  and `tddy-tools`; `tddy-daemon`'s tests additionally shell out to `tddy-remote-git-repo`,
  `tddy-session-sync` and `tddy-sandbox-runner`. Without them **13 tests** fail with
  `tddy-remote-git-repo is not built at target/debug/tddy-remote-git-repo; build it before running
  this suite`. Building the two missing binaries took the failure count from **38 to 18** with no
  code change. The message names the cause, so this reads as a fixture problem rather than the
  harness gap it is.

Not fixed in #466 because that branch is `tddy-code-analysis` work and these are root-script
changes with their own blast radius — `./test` is what every other agent and hook in the repo
depends on. Worth its own PR, and the exit-code half should land first: until it does, no `./test`
result anywhere can be trusted.
