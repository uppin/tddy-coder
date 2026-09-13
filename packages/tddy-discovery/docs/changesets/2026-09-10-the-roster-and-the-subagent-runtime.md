# 2026-09-10 — The live agent roster and the subagent conversation runtime move in

**Type:** Architecture

Node 5 of the `#unbundle` stack ([#474](https://github.com/uppin/tddy-coder/pull/474)). Full story in
the cross-package entry:
[2026-09-10-unbundle-tools-thinning.md](../../../../docs/dev/changesets/2026-09-10-unbundle-tools-thinning.md).

Two public modules arrive from `tddy-tools`: **`roster`** (all five former `session_agents` modules,
1,802 lines) and **`subagent_runtime`** (seam D's conversation table, 549 lines). Documented in
[roster-and-subagent-runtime.md](../roster-and-subagent-runtime.md).

**All five roster modules moved together, because they cannot split.** The plan gave four to
`tddy-service` and left `registry.rs` here. `registry.rs` imports three `connection` protos, so its
crate depends on `tddy-service`; `seed.rs` and `stream.rs` import `registry::LiveAgentRoster`, so
theirs is registry's crate or one above it — and `tddy-service` is *below* it; `conversation.rs`
implements this crate's own `subagent::SubagentSession`. Only a crate above both `tddy-service` and
this crate's types can hold all five, and that crate is this one.

The seam within seam D followed the same rule as every other in the node: **430 production lines of
runtime** — the conversation table, the turns that outlive their calls, token accounting, the status
report to the facilitating daemon — are logic over this crate's `SubagentSession`, `PromptOutcome`,
`TokenUsage` and `StopReason`, and moved; **779 lines of MCP surface** stayed in `tddy-tools`,
because they are what `rmcp` sees. The runtime's table and its `PendingTurns` are therefore `pub`
here: the tool bodies that stayed drive them.

**`RosterCurrency` stays private.** It has four states and only some may still enforce a tool
withdrawal; callers read the answers instead (`RosterStatusReport::{applied_rev, refusal}`, and the
refusals from `resolve` and `check_tool_available`). The red phase declared a public two-state
version and its own test asserted `Current` for a fresh roster the real design calls `Seeded` —
exactly the conflation the four states exist to prevent — so the stub and its three tests were
deleted rather than matched.

**The `registry`/`seed` mutual import was unpicked** rather than carried across. It was legal inside
one crate and would have stayed legal here, so this was not forced: it was taken because the pair had
one caller between them. `seed_agent_id` and `seed_entry` are now private to `registry.rs` (+57
lines), `seed.rs` imports `registry` and `registry` imports nothing back, and each module keeps the
property its doc claims — "what the environment claimed" in one file, "what the roster says" in the
other.

**Dependencies gained**: `tddy-service` (the roster protos), `tddy-session-tool-client` (how a jail
reaches its facilitating daemon), `tddy-rpc`, `prost`, `uuid`, and `tokio`'s `sync`/`time`. A new
**default-off** `livekit` feature forwards to the tool client's.

⚠ **`tddy-service` depends on `tddy-tui`**, so this crate's **eight** reverse-dependencies now build
the TUI transitively — the in-jail ones to follow a roster. There was no move that avoided it;
[recorded](../../../../docs/dev/todo/2026-09-10-three-crates-gained-a-transitive-tddy-tui-dependency.md)
with the two other crates that acquired the same edge in this node, all three fixed by splitting the
protos out of `tddy-service`.

Log targets moved with the code: `tddy_tools::session_agents` is now `tddy_discovery::roster` (12
sites) and `tddy_discovery::subagent_runtime` (2). An operator watching the roster stream wants
`RUST_LOG=tddy_discovery=debug`.

Tests: 109 / 0. The heaviest coverage stays in `tddy-tools`, driving the real `--mcp` stdio wire —
83 tests across four suites through `tddy_tools::session_agents`, now a re-export of `roster`.
