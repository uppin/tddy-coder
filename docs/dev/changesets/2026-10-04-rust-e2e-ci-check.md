# 2026-10-04 — The Rust e2e tests run as their own CI check

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Guide:
[Continuous integration](../guides/ci.md#rust-tests-and-rust-e2e-tests).

`Rust tests` is split in two checks by what a test needs: `Rust tests` (everything that runs in
process) and `Rust e2e tests` (tests that need a LiveKit server in Docker, or a real
`tddy-supervisor`, index-daemon, daemon or `tddy-coder` process). One matrix job runs both legs, from
one filterset, `.config/rust-e2e.filterset`, which each leg takes or negates; each leg publishes its
own report and JUnit artifact.

- The set is 41 filterset terms: 35 LiveKit-backed test binaries (plus the testkit's own), the
  supervisor's integration binaries, and four daemon-class binaries — `dual_transport_acceptance`,
  `ping_answers_only_a_live_listener`, `index_daemon_client_acceptance`, `acceptance_daemon` — and
  `stdio_remote_control_acceptance`.
- The `docker` test-group in `.config/nextest.toml` had drifted: seven of its entries named a package
  the test left in the daemon carve, and five LiveKit binaries were never listed. They are now carried
  by binary name, so the e2e leg's LiveKit tests no longer run in parallel on one port range.
- `Rust e2e tests` is not made a required check here; that is branch protection.

**Verified.** The workflow parses; `nextest.toml` and the filterset parse and, on `tddy-supervisor`,
the two legs partition the package (5 integration binaries in the e2e leg, the 176 lib unit tests in
the other). The full workspace partition and the new `docker` entries have **not** been run locally —
that is a CI run.
