# 2026-10-08 — Drain `tddy-discovery` into purpose-named crates and remove it

**Category:** Future enhancement (crate responsibility and size)
**Source:** #carve 21/21 (PR #536) — LoC and responsibility survey of `packages/`, 2026-10-08

`tddy-discovery` began as a one-shot codebase-exploration agent. It is now the home of the whole
specialized-subagent runtime: ~11.6k raw lines in `src/` (inline tests included), **~9.1k production
lines** (brace-depth strip), 14 dependent packages. It is already under the 10k target, so the case
for draining it is its **responsibility spread**, not its size. Only ~3 % of it (the citation parser) is what the name describes. The developer's direction
(2026-10-08): treat it as a package to **drain**, and remove it once empty — it can be removed fully,
because nothing in it needs to stay under that name.

## What it holds, and where each part goes

Raw lines. Consumers are the packages that name `tddy_discovery::<module>` (measured 2026-10-08).

| Lines | Modules | Responsibility | Consumers | Proposed destination |
|---:|---|---|---|---|
| ~5.0k | `subagent.rs`, `subagent/` | stateful `SubagentSession`s exposed over MCP: turns, transcripts, yield conditions, repeated-call detection, per-conversation worktree glue | acp, daemon-rpc, sandbox-app, sandbox-runner, session-agents, session-lifecycle, session-split, tool-engine, tools | `tddy-subagent` (new) |
| ~3.6k | `roster.rs`, `roster/`, `subagent_runtime.rs`, `subagent_runtime/` | in-jail roster registry; every open conversation, its running turns and spend (moved here from `tddy-tools` by `#unbundle` node 5) | sandbox-app, sandbox-runner, session-agents, session-split, tools | `tddy-subagent` (new), or its own crate if that exceeds 10k production lines |
| ~1.5k | `openai.rs`, `backend.rs` | OpenAI-compatible chat client; one-shot `SpecializedAgentBackend: CodingBackend` loop | acp, session-agents (`openai`); coder (`backend`) | `tddy-agent-model-loop` (new) |
| ~0.5k | `tools.rs`, `discovery.rs` | READ/GLOB/GREP executor (local or via `ExecToolService`); citation-line parser | internal only | `tddy-agent-model-loop` |
| ~0.6k | `agent_def.rs`, `warmup.rs` | YAML `SpecializedAgentDef` (`<tddyhome>/agents/*.yaml`), the single config surface; start-time endpoint warm-up gate | `agent_def`: 11 packages; `warmup`: daemon-kernel, sandbox-app, session-lifecycle | `tddy-agent-defs` (new) |
| ~0.2k | `catalog_service.rs`, `catalog_entry.rs` | `catalog.CatalogService` adapter over a host-wired `CatalogHandler` | daemon-rpc | `tddy-session-catalog` (exists, 1.5k) or `tddy-daemon-rpc` |
| ~0.1k | `agent_list_mapping.rs` | agent-list mapping | daemon-kernel | check against `tddy-daemon-kernel`'s own `agent_list_mapping` (lifecycle's copy moved there in #carve 21/21 R1): two modules with one name is a duplication to resolve, not to move |

`agent_def` is the widest-used surface (84 references across 11 packages), so it is the first module
to move and the one that most needs a `pub use` facade during the drain.

## Suggested order (each step a node of its own)

1. `agent_def` + `warmup` → `tddy-agent-defs`. Widest fan-in, smallest, and nothing below it.
2. `openai` + `backend` + `tools` + `discovery` → `tddy-agent-model-loop`.
3. `subagent` + `roster` + `subagent_runtime` → `tddy-subagent`.
4. `catalog_service` / `catalog_entry` / `agent_list_mapping` to their owners.
5. Delete `tddy-discovery` and re-point its 14 dependents' manifests.

Each step leaves `pub use` re-exports in `tddy-discovery` until step 5, so no consumer is edited
mid-drain. Moves go through the `tddy-tools restructure` engine; refusals stop and ask.

## Open questions

- **Name `tddy-subagent`** versus folding it into `tddy-session-agents` (which already needs
  `tddy-subagent-worktree`'s `ToolEffect` / `ConversationId`, see #536 R6). The roster is in-jail and
  `session-agents` is daemon-side, so merging them would put in-jail code on the daemon's side of the
  graph: keep them apart unless the dependency check says otherwise.
- **Does `subagent` + `roster` + `subagent_runtime` fit under 10k production lines?** ~8.6k raw; the
  per-module production figures are not yet measured (the first attempt at measuring them failed on a
  quoting error in the counting loop; only the crate total, ~9.1k, is valid).
- **`tddy-core` and `tddy-service` dependencies** (the crate's own edges): which destination crates
  inherit them.

## Why this was deferred

Found while surveying crate sizes during #536, a move node on a different crate. It is its own
changeset (edge table, baselines, facade list) and belongs after the `#carve` moves land. It
overlaps `tddy-subagent-worktree` and `tddy-session-agents`, whose boundaries #536's R6 is still
settling. Related: [split `tddy-code-restructuring`](2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md).
