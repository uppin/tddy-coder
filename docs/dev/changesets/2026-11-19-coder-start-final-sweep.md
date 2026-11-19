# 2026-11-19: The `#coder-start` final sweep

**Type:** Refactor · `#coder-start` 10/10, PR [#952](https://github.com/uppin/tddy-coder/pull/952)
**Packages:** `tddy-agent-launch`, `tddy-session-lifecycle`

This sweep pays down what #944–#949 left, with no behaviour change.

| Item | Before | After |
|---|---|---|
| `tddy-agent-launch` `start_cli_harness_session` | 397 (383 at the stack's base `8b004d090`) | **371**: four engine `extract_method`s, each helper ≤ 22 lines and ≤ 5 parameters; the record is narrowed and stays open (budget 60) |
| target-session test fixtures | identical copies in four suites | two support modules: `tddy-session-lifecycle/tests/derived_host_support`, `tddy-agent-launch/tests/acme_target_support` (−640/+37 in the suites) |

- **Not cut:** the target-shell resolution. `check --deep` refused it because its signature names
  `CoderCredentialSource`, which `opencode_cli_spawn.rs` does not import. It was not unlocked with a
  hand-written import.
- **Backlog narrowed:** the duplicated-fixtures entry now lists only copies that have drifted. The
  real-deployment entry gained `stopped`, read read-only from the real deployment's
  `/api/v2/workspaces` (`latest_build.status`). The hook follower is still unverified.
- **New backlog entry, a product question:**
  [a target start requires the agent on the daemon's host](../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md).
- **Left open:** `launch_on_target` keeps its 6 parameters, because bundling them changes a
  signature `tddy-session-driver` calls. The idle-session read oddities were never reproduced.
- The stack added no `TODO`/`FIXME` marker.

Verified, scoped: clippy `-D warnings --all-targets` and `cargo fmt --check` were clean on both
packages. `restructure verify --against` lost no tokens. The comment-line count of the touched source
was unchanged (51). The packages' test suites were **not run locally**, because the machine was at
its disk floor, so test health is CI's.

Per-package notes: [agent-launch](../../../packages/tddy-agent-launch/docs/changesets/2026-11-19-coder-start-final-sweep.md),
[session-lifecycle](../../../packages/tddy-session-lifecycle/docs/changesets/2026-11-19-coder-start-final-sweep.md).
