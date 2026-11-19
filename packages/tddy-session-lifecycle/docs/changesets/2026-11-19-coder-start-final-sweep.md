# The derived-host daemon fixture is shared

**Date:** 2026-11-19
**Type:** Refactor (tests only)
**Packages:** tddy-session-lifecycle

`tests/derived_host_support` holds the daemon `derived_host_start_acceptance` and
`orchestrator_target_start_acceptance` both start sessions on: the operator home, the stub agent, the
host service and `ADaemonReachingAnSshHost` with its listing, run, target and stop helpers. The two
suites had carried it byte for byte, apart from the project id, which is now a parameter. Each suite
keeps its own start; the assertions are unchanged.
