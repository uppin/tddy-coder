# 2026-10-24 — A `warp-cli` session cannot run in the sandbox

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Boundaries

A sandboxed `warp-cli` start is refused `FAILED_PRECONDITION` ("the sandbox runner does not host
warp-cli: warp cannot run in the jail: its spec's placements are [host]"). Since `#agent-tools` 8/12
(#797) the refusal is spec data: `packages/tddy-coding-agent-specs/specs/warp.yaml` declares
`placements: [host]`, read by `harness_from_spec::require_spec_placement`. The developer decided on
2026-10-31 that Warp stays refused for now.

**Why deferred:** out of the node's scope; jailing Warp also needs its sign-in state reachable from
inside the jail, and the resume-token recorder reading the jailed terminal's output.

**What would close it (narrowed 2026-11-03, #797):** in `warp.yaml`, a `files`/`reads` entry for
Warp's config directory (its sign-in state) and the jail sections (`home`, `policy`, `network`); the
`warp-resume-token` adapter wired on the jailed launch (`launch_jailed` has no `on_exit` yet); and
`placements: [host, jail]`. Then turn the refusal tests
(`tddy-session-lifecycle/tests/warp_cli_session.rs`,
`harness_clis_from_spec_acceptance::a_sandboxed_warp_cli_start_is_refused_failed_precondition_naming_the_placements`)
into start tests.
