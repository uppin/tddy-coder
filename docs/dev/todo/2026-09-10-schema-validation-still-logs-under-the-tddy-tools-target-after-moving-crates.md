# 2026-09-10 — Schema validation still logs under a `tddy_tools::` target after moving crates

**Category:** Known defect
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M9 verification

M4 moved `schema.rs` and `schema_manifest.rs` from `tddy-tools` to `tddy-workflow-recipes`. Their
`log` targets did not move with them: nine sites in
`packages/tddy-workflow-recipes/src/schema.rs` and `schema_manifest.rs` still emit
`target: "tddy_tools::schema"` and `target: "tddy_tools::schema_manifest"`.

**What makes this an entry rather than a policy is that node 5 is internally inconsistent about it.**
Its later milestones moved the target with the code and each recorded the operator-visible
consequence: `tddy_tools::session_tool_client` → `tddy_session_tool_client` (M7),
`tddy_tools::session_agents` → `tddy_discovery::roster` and `tddy_discovery::subagent_runtime`
(M8, 14 sites). M4 is the one milestone of the five that did not, so two modules out of the node's
twenty-four now disagree with the rest of the node.

Note that **node 4 took the opposite policy deliberately** and recorded it in
[`2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md`](./2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md):
keep the old target, because renaming silently breaks an operator's existing `RUST_LOG` filter and
the failure mode is *missing logs*. That reasoning applies here too — which means the thing to fix
is not necessarily these nine sites but **the stack's disagreement about which rule it follows**.
The `#unbundle` stack currently ships both.

Operator-visible either way: today `RUST_LOG=tddy_workflow_recipes=debug` shows nothing from schema
resolution or validation, while `RUST_LOG=tddy_tools=debug` still shows it from a crate that no
longer contains it. Errors are unaffected — they are emitted at `error` and pass the default `warn`
filter whatever the target.

Whichever rule wins, the fix here is two target strings plus the paragraph that documents them,
`packages/tddy-tools/docs/json-schema.md:35`, which names both targets and which the same changeset
wants moved to `tddy-workflow-recipes/docs/`. Not taken at M9 because M9 is verification, and this
needs one decision covering the whole stack rather than a rename smuggled into a closeout.
