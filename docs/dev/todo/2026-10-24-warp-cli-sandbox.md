# 2026-10-24 — A `warp-cli` session cannot run in the sandbox

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Boundaries

A sandboxed `warp-cli` start is refused `FAILED_PRECONDITION` ("the sandbox runner does not host
warp-cli"): `sandboxed_agent_kind` in `packages/tddy-agent-launch/src/cli_harness_jail.rs` has no
Warp arm, and the sandbox runner, recipes and app know no Warp.

**Why deferred:** out of the node's scope; jailing Warp also needs its sign-in state reachable from
inside the jail, and the resume-token recorder reading the jailed terminal's output.

**What would close it:** a `warp-cli` agent kind in the runner (argv from `WARP_CLI`), the jail's
allow-list for Warp's config directory, the recorder wired on the jailed launch, and the refusal test
in `tddy-session-lifecycle/tests/warp_cli_session.rs` turned into a start test.
