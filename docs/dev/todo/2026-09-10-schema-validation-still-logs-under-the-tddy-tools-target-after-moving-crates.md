# 2026-09-10 — M4's moved modules still log under a `tddy_tools::` target after moving crates

**Category:** Known defect
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M9 verification

M4 moved `schema.rs`, `schema_manifest.rs` and `github_pr.rs` from `tddy-tools` to
`tddy-workflow-recipes`. Their `log` targets did not move with them: **22 sites across three
modules** in `packages/tddy-workflow-recipes/src/` still emit a `tddy_tools::` target.

| Module | Sites | Target still emitted |
|---|---:|---|
| `schema.rs` | 6 | `tddy_tools::schema` |
| `schema_manifest.rs` | 3 | `tddy_tools::schema_manifest` |
| `github_pr.rs` | 13 | `tddy_tools::github_pr` |

`github_pr.rs`' sites are lines 119, 136, 143, 152, 174, 182, 191, 214, 228, 247, 259, 274, 284 —
the same M4 move, found after this entry was first written, which had counted only the two schema
modules.

**What makes this an entry rather than a policy is that node 5 is internally inconsistent about it.**
Its later milestones moved the target with the code and each recorded the operator-visible
consequence: `tddy_tools::session_tool_client` → `tddy_session_tool_client` (M7),
`tddy_tools::session_agents` → `tddy_discovery::roster` and `tddy_discovery::subagent_runtime`
(M8, 14 sites). M4 is the one milestone of the five that did not, so three modules out of the
node's twenty-four now disagree with the rest of the node.

Note that **node 4 took the opposite policy deliberately** and recorded it in
[`2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md`](./2026-09-10-log-targets-across-the-extracted-crates-still-name-tddy-daemon.md):
keep the old target, because renaming silently breaks an operator's existing `RUST_LOG` filter and
the failure mode is *missing logs*. That reasoning applies here too — which means the thing to fix
is not necessarily these nine sites but **the stack's disagreement about which rule it follows**.
The `#unbundle` stack currently ships both.

Operator-visible either way: today `RUST_LOG=tddy_workflow_recipes=debug` shows nothing from schema
resolution, validation or the GitHub PR REST client, while `RUST_LOG=tddy_tools=debug` still shows
all three from a crate that no longer contains them. Errors are unaffected — they are emitted at `error` and pass the default `warn`
filter whatever the target.

Whichever rule wins, the fix here is three target strings plus the paragraph that documents two of
them,
`packages/tddy-tools/docs/json-schema.md:35`, which names both targets and which the same changeset
wants moved to `tddy-workflow-recipes/docs/`. Not taken at M9 because M9 is verification, and this
needs one decision covering the whole stack rather than a rename smuggled into a closeout.
