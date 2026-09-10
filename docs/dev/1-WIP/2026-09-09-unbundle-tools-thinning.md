# Changeset: `tddy-tools` becomes CLI dispatch and an MCP router

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#unbundle` node **5 of 8**. PR [#474](https://github.com/uppin/tddy-coder/pull/474).
Base: `feature/unbundle/auth-livekit` (node 4, PR #473)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-tools-thinning-initial-discovery.md](./2026-09-09-unbundle-tools-thinning-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

`tddy-tools` goes from 28 modules / 10,467 prod LoC to **four things**: `main.rs`'s argument parsing
and dispatch, the clap arg structs, the Claude Code permission-decision engine, and the `rmcp`
`ServerHandler` that assembles the tool router. Everything else goes to the crate that already owns
its domain.

**Step zero — break the three import cycles with `server.rs`.** `action_tools`, `lsp_tools` and
`session_agents/{seed,stream}` all import back from `server`, over eight small shared items
(`env_non_empty`, `schema_object`, `subagent_route`, `subagent_error_json`, `RemoteToolDef`,
`seed_subagents_or_report`, `open_roster_agent_session`, `cancel_remote_conversation`). One shared
`mcp_primitives` module breaks all three, and **nothing else in this node can move until it does**.

| Destination | What moves | prod LoC |
|---|---|---:|
| `tddy-workflow-recipes` | `github_pr.rs`, `schema.rs`, `schema_manifest.rs`, `review_persist.rs` | ~940 |
| `tddy-service` | `session_tool_client.rs`, `session_agents/{stream,conversation,link,seed}` | ~1,900 |
| `tddy-discovery` | `server.rs` seam D (the subagent conversation runtime), `session_agents/registry.rs`, `relay.rs` | ~2,180 |
| `tddy-tool-engine` | `server.rs` seam C (the dynamic tool proxy and `exec_tool_catalog`) | ~190 |
| `tddy-terminal-rpc` | `pty_relay.rs` | 843 |
| `tddy-core` | `toolcall_client.rs`, `session_context.rs`, `cli.rs`'s wire types and relay pairs, and the non-CLI half of `action_tools.rs`, `list_models.rs` and `session_actions_cli.rs` (`session_hook.rs` cannot move — cycle) | ~700 |
| `tddy-bsp` | `build_cli.rs`'s dispatch | 194 |
| `tddy-code-analysis` | `analyze_cli.rs`'s dispatch | 86 |
| `tddy-code-restructuring` | `restructure_cli.rs`'s dispatch | 147 |
| `tddy-lsp-executor` | `lsp_tools.rs` | 137 |

**Three duplications are deleted rather than relocated:**

- `build_cli::plugin_registry()` (`build_cli.rs:27-35`) registers the same five build plugins as
  `tddy_bsp::plugins::plugin_registry` — **verbatim**. Moving the dispatch into `tddy-bsp` deletes one
  of them, and lets `tddy-tools` drop **all six** `tddy-build*` dependencies.
- `server::exec_tool_catalog()` is a hand-copied `RemoteToolDef` clone of
  `tddy_tool_engine::catalog::tool_catalog()`, guarded by matched tests in both crates
  (`server.rs:1578-1579` names them). Moving seam C into `tddy-tool-engine` makes it one catalog.
- `github_pr.rs` already re-exports from `tddy_workflow_recipes::github_rest_common`; moving it there
  removes the indirection, and `review_persist.rs` — 15 lines delegating to
  `tddy_workflow_recipes::review::persist_review_md_to_session_dir` — is deleted outright.

**`restructure_cli::cli_vector()` is deleted.** It re-serializes parsed clap arguments back into a
`Vec<String>` to hand to `tddy_code_restructuring` — direct evidence that the CLI boundary is in the
wrong place. With the dispatch in `tddy-code-restructuring`, the parsed args are used as parsed.

**`tddy-tools`' `build.rs` is deleted.** It generates `OUT_DIR/goal_registry.rs` from
`../tddy-workflow-recipes/goals.json`, and `schema.rs`/`schema_manifest.rs` reach into that sibling
package's directory at compile time with `include_dir!` and `include_str!`. Once those two modules
live in `tddy-workflow-recipes`, the cross-package compile-time reach and the build script both go.

**The headline outcome, asserted rather than described:** `tddy-daemon`, `tddy-sandbox-app` and
`tddy-sandbox-darwin` **drop their dev-dependency on `tddy-tools` entirely**. That dep exists only for
`session_tool_client::{dispatch_via_sandbox_ipc, dispatch_session_tool}` and
`session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`, all three of which move to `tddy-service`.

## Boundaries

This PR explicitly does **not**:

- Change any proto, or touch `connection.ConnectionService`. `tddy-tools` is a *client* of it; the
  method coordinates it names are unchanged here. Families A, L and P move in nodes 6–8.
- Move seam A (the permission-decision engine, ~250 LoC) or seam E (the `ServerHandler` router
  assembly, ~230 LoC) out of `tddy-tools`. Those two **are** the MCP server, which is why the crate
  exists. Seam A also carries a bespoke newline-delimited-JSON Unix-socket protocol that
  `toolcall_client` deliberately migrated away from; relocating it would spread that protocol rather
  than retire it.
- Move seam B (the 14 `pr_*` + 2 `github_*` MCP tools, ~415 LoC with their `Pr*Input` structs) out
  of `tddy-tools`. **Changed during green — the plan said it moved.** The bodies turned out to be
  thin adapters that already delegate into `tddy_workflow_recipes::pr_stack`
  (`pr_merge_action`, `pr_close_action`, `set_internal_status`, `AddPlannedPrInput`, …) and wrap the
  result in a JSON envelope; the logic was never in `tddy-tools` to move. What is left is MCP
  advertisement, and moving that would mean adding `rmcp` and `schemars` to `tddy-workflow-recipes`,
  which has neither — every consumer of that crate, `tddy-coder` included, would then build `rmcp`.
  The same rule that keeps seams A and E here keeps seam B: the shape of an MCP tool belongs to the
  crate that speaks MCP. M3 already applied it when `lsp_tool_catalog()` kept returning `LspToolDef`
  rather than `RemoteToolDef`.
- Move `session_hook` into `tddy-core`, or move `list_models` / `session_actions_cli` there whole.
  **Changed during green — the plan said all six moved.** Three distinct outcomes, not one:
  - **`session_hook` cannot move at all.** It imports `tddy_service::proto::connection`, and
    `tddy-service` depends on `tddy-core`. That is a dependency **cycle**, not a cost, and no
    dependency addition fixes it. It stays in `tddy-tools` whole.
  - **`session_context` moved.** Its only blocker was `anyhow`, which `tddy-core` did not have.
    Adding it was a deliberate, developer-authorised decision (see `## Decisions & Trade-offs`);
    the module then moved verbatim, 66 lines.
  - **`list_models` and `session_actions_cli` are split, not moved.** `anyhow` alone did not
    unblock them: both parse clap arguments, both `println!` a JSON contract, and
    `session_actions_cli` exits with a classified code. `tddy-core` is a library the TUI depends
    on, and CLAUDE.md forbids direct stdout in any path that runs under the TUI — moving those
    functions into it would put that hazard somewhere far more dangerous than a binary crate. So
    the logic moved and the surface stayed: `list_models` 172 → 52 with the catalogue assembly and
    its JSON contract in `tddy_core::backend::model_catalog` (159), beside the backends it
    enumerates; `session_actions_cli` 136 → 66 with the session-directory resolution in
    `tddy_core::session_actions::session_dir` (105).

  The rule throughout is seam B's, one level down: the shape of a CLI subcommand belongs to the
  crate that parses arguments. Earns a `## Scope` line and a milestone note.
- Move `pty_relay`'s clap surface, or seam C's MCP surface, out of `tddy-tools`.
  **Changed during green at M6 — the plan said both moved whole.** Same rule, twice more:
  - **`PtyRelayArgs` stays.** It is a `#[derive(Args)]` struct with twenty `#[arg]`s and their
    default values, and `tddy-terminal-rpc` has no `clap`. The relay itself — all four dispatch
    modes, 698 non-blank lines — moved; `tddy-tools` keeps 142 that parse arguments and build a
    `PtyRelayConfig`. **PR #475 (node 6) compiles against `tddy_terminal_rpc::pty_relay`**, and it
    is the relay it needs, not the flags.
  - **Seam C's MCP half stays.** `build_dynamic_tool_list` returns `Vec<rmcp::model::Tool>` and
    `dynamic_tool_router` returns an `rmcp` `ToolRouter`; `static_tool_names` names the MCP
    server's own two always-registered tools, `approval_prompt` and `submit`, which are seams A
    and E. Moving any of them would put `rmcp` into `tddy-tool-engine`, a crate every
    workspace-session host links. `dispatch_dynamic_tool` is M7's — it resolves the call against
    the live agent roster. What moved is what needed nothing: the catalog and
    `is_native_tool_denied_in_remote_mode`.
- Retire that NDJSON protocol. Recorded in `docs/dev/todo/` at wrap.
- Move any `tddy-daemon` module. Nodes 1–4 did the daemon; nodes 6–8 do the rest.
- Change the 25 `TDDY_*` environment variables that are `tddy-tools`' real hidden interface
  (`TDDY_SOCKET` ×43, `TDDY_REPO_DIR` ×24, `TDDY_SESSION_DIR` ×12). Each moved module carries its env
  contract verbatim; renaming any of them is out of scope and would break in-jail agents.
- Force every file under 500 lines. `server.rs` (3,842), `session_tool_client.rs` (991), `cli.rs`
  (927), `pty_relay.rs` (843) and `session_agents/registry.rs` (685) are over budget; the seams above
  bring most of them under it as a side effect, and whatever stays over is recorded in `## Scope`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate` | every move here is a plan the operation executes — 11 destinations, the largest fan-out in the stack | add, extend or fix the operation |
| `n1` host-worktree-services | the `edits_for` fix, so a rename re-points callers in other files | `tddy-tools`' modules are reached from its own `cli.rs` and `main.rs`, and those references are re-pointed rather than hand-edited | rely on the old single-document behaviour |
| `n1` host-worktree-services | `restructure_cli.rs`'s CLI surface for the new operation | this PR **moves that file into `tddy-code-restructuring`**, so it inherits node 1's version and must not revert its additions | drop or rewrite the `move_module_to_crate` CLI surface while relocating the file |
| `n2`, `n3`, `n4` | nothing this PR consumes | — | — |

`n2`–`n4` are branch ancestors, not dependencies. **One collision to watch**: node 3 moved
`bsp_service.rs` into `tddy-bsp` and this node moves `build_cli.rs`'s dispatch there too. Both edit
`packages/tddy-bsp/src/lib.rs`. Expect a conflict on the cascade and keep both additions.

## Draft PR contract

What lands in this PR's **second commit**, and it matters more here than anywhere else in the stack
because **nodes 6, 7 and 8 all compile against it**:

- `packages/tddy-tools/src/mcp_primitives.rs` — the eight shared items that break the three cycles.
  This is the unblocking move; without it nothing else in the node compiles.
- In `tddy-service`: `session_tool_client`'s public surface — `SessionToolTransport`,
  `dispatch_session_tool`, `dispatch_via_sandbox_ipc`, `LiveKitRoomCache`,
  `PASS_LONG_ENOUGH_TO_BE_SERVICE` — with real signatures. **Node 7 serves the roster and conversation
  families through these**, so their shape is fixed here.
- In `tddy-tool-engine`: the single `tool_catalog()` and `dynamic_proxy::
  is_native_tool_denied_in_remote_mode`. **Node 8 serves families A and L from this crate**, so its
  surface is fixed here. **Corrected at M6 — the first push declared `RemoteToolDef`,
  `build_dynamic_tool_list(host_tools, withdrawn)` and `dispatch_dynamic_tool` here.** The first
  was a fourth copy of `ToolDef`'s three fields (and named its schema field `input_schema`, not
  `input_schema_json`); the second returns `Vec<rmcp::model::Tool>` and never had a `withdrawn`
  parameter — withdrawal is enforced at dispatch against the live roster, not by filtering the
  advertisement; the third is M7's, because that roster is a `tddy-service` concern. Node 8 builds
  against `tool_catalog()`, `execute_tool` and the denial predicate; the MCP shape stays in
  `tddy-tools`.
- In `tddy-terminal-rpc`: `pty_relay::{PtyRelayConfig, run_pty_relay}` — one plain config struct,
  one `async fn … -> anyhow::Result<()>`, four dispatch modes selected by which fields are set.
  **Node 6 serves family K from this crate.** **Corrected at M6 — the first push declared a
  `RelayMode` enum, a `RelayError` and `run_pty_relay(RelayMode)`.** None of the three existed;
  the modes are not disjoint at the CLI (`--sandbox`, `--model` and `--daemon-url` are read by
  more than one), so an enum would have had to repeat their fields per variant.
- In `tddy-discovery`: the subagent conversation runtime's public surface and `LiveAgentRoster`.
- Every destination's `Cargo.toml` change, so `cargo build --workspace` sees the new surfaces.
- The failing acceptance tests, including the three dependency-drop assertions.

Bodies annotated `// TODO(tools-thinning): implement`.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** 2 of 3
**Greenable independently:** yes — once node 1 is green. Nothing here needs a daemon subsystem crate;
`tddy-tools`' 50 test files drive it through its own CLI and its own `--mcp` stdio wire.
**Concurrent with:** nodes 2 and 3. Node 4 is wave 3 for its own reason (it needs node 2).
**Blocks:** **nodes 6, 7 and 8.** They serve their proto families from `tddy-service`,
`tddy-terminal-rpc` and `tddy-tool-engine` — the three surfaces this node moves into place. Starting
any of them before this node's contract is pushed means building against a surface that is about to move.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8

⚠ **Recurring conflicts**: `packages/tddy-bsp/src/lib.rs` with node 3; `packages/tddy-tools/src/lib.rs`
and `main.rs` with nothing (this node owns them). No `runtime.rs` edit here — the first node in the
stack that does not touch it.

## Affected Packages

- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — 24 of 28 modules leave; `build.rs`
  and the second `[[bin]]`'s forced dependencies go; 17 of 34 dependencies drop
  - [json-schema.md](../../packages/tddy-tools/docs/json-schema.md) → `tddy-workflow-recipes/docs/`
- **tddy-workflow-recipes**, **tddy-service**, **tddy-discovery**, **tddy-tool-engine**,
  **tddy-terminal-rpc**, **tddy-core**, **tddy-bsp**, **tddy-code-analysis**,
  **tddy-code-restructuring**, **tddy-lsp-executor** — each gains the modules for the domain it owns
- **tddy-daemon**, **tddy-sandbox-app**, **tddy-sandbox-darwin**: their `tddy-tools` dev-dependency is
  **removed**
- **tddy-testing-commons**: has `tddy-tools` in `[dependencies]` while `tddy-tools` has it in
  `[dev-dependencies]` — an existing prod/dev cycle that must be resolved before the surface moves
- **tddy-service**: no proto change

## Related Feature Documentation

**None — behaviour-preserving restructure. No PRD.** No RPC coordinate moves, no client migrates, no
observable behaviour change: the CLI surface, the 43 advertised MCP tools and the 25 `TDDY_*` env
variables are all identical afterwards. Following the precedent of
`docs/dev/1-WIP/2026-09-09-connection-service-split.md`.

## Summary

`tddy-tools`' 24 non-wiring modules move to the ten crates that already own their domains, three
duplications are deleted rather than relocated, and three crates drop their dependency on
`tddy-tools` entirely. What remains is CLI dispatch, the permission engine and the MCP router.

## Background

`main.rs` (170 lines) is already exactly what this node aims for — argument parsing and dispatch. The
problem is `lib.rs`'s 14 public modules and a 3,842-line `server.rs` that is three things at once: an
`rmcp` MCP server over stdio, a Claude Code `--permission-prompt-tool` decision engine, and an
MCP→daemon proxy.

Two facts make this the cheapest large move in the stack:

- **`tddy-tools` has essentially no production consumers.** Four API surfaces are reached from outside
  it, all from *tests*, and four of its five reverse-dependencies are `[dev-dependencies]`. Moving
  code out of it is therefore almost free at the caller boundary — the exact opposite of the daemon's
  situation.
- **Every destination already exists and already owns the domain.** `tddy-workflow-recipes` owns
  `orchestrate_pr_stack`, `pr_stack` and `github_rest_common` *and* the `goals.json` that `schema.rs`
  compiles in; `tddy-tool-engine` owns the same ten-tool catalog; `tddy-discovery` owns every type the
  subagent runtime manipulates; `tddy-terminal-rpc`'s own description already names `tddy-tools` as a
  consumer and it already owns `local_pty_relay`. Fourteen of the seventeen destinations are already
  `tddy-tools` dependencies, so most moves add no Cargo edge at all.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — the `tddy-testing-commons` ↔ `tddy-tools` prod/dev cycle

`tddy-testing-commons` has `tddy-tools` in `[dependencies]` (the only non-dev dependent anywhere)
while `tddy-tools` has `tddy-testing-commons` in `[dev-dependencies]`. Every route around it is wrong:
moving `tddy-tools`' public surface while that cycle stands either breaks
`tddy-testing-commons`' build for every consumer of it, or forces the moved modules to be re-exported
from `tddy-tools` purely to keep the cycle alive — which is the opposite of this node's purpose. It is
resolved first, by repointing `tddy-testing-commons` at the destination crates. Earns a `## Scope` line.

### ⛔ BLOCKING — `2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md`

The action tools are advertised where nothing implements them. `action_tools.rs` moves to `tddy-core`
in this PR, and relocating an advertisement whose implementation does not exist would carry a known
false advertisement into a new crate and make it harder to find. The entry is resolved as part of the
move: either the tools are implemented where they are advertised, or the advertisement is withdrawn.
Earns a `## Scope` line.

**Resolved at M5 by withdrawal** — see `## Decisions & Trade-offs`. Close the entry at wrap.

### ⚠ DURING — `2026-08-13-session-action-wait-times-out-while-running-is-load-sensitive-resolved.md`

A load-sensitive test in the session-action path, which moves here. The move must not change its
timing characteristics; if it becomes flakier in its new crate, that is this node's regression to fix
rather than an inherited one.

### ℹ ANSWERED — `2026-09-06-server-rs-run-server-takes-12-positional-arguments.md`

Node 1 fixed the daemon's `run_server`. This entry's sibling problem in `tddy-tools` —
`restructure_cli::cli_vector()` re-serializing parsed clap arguments back into strings to cross a
package boundary — is answered here by moving the boundary instead of the strings. Close both at wrap.

## Scope

- [x] **⛔ Resolve the `tddy-testing-commons` ↔ `tddy-tools` cycle** before any surface moves ✅
- [x] **Step zero — `mcp_primitives`**: the eight shared items; all three `server.rs` cycles broken ✅
- [x] **⛔ Action-tool advertisement**: **withdrawn** — merged only when the host claims the surface
      (`TDDY_SESSION_ACTION_TOOLS`, the `TDDY_LSP_TOOLS` shape). Advertised set is unchanged (43) on a
      transport that serves them and **three fewer (40)** on the daemon path ✅
- [x] **`tddy-workflow-recipes`**: `github_pr`, `schema`, `schema_manifest`; `review_persist` deleted ✅ (seam B stays — see `## Boundaries`)
- [ ] **`tddy-service`**: `session_tool_client` + 4 `session_agents` modules
- [ ] **`tddy-discovery`**: seam D, `registry.rs`, `relay.rs`
- [x] **`tddy-tool-engine`**: seam C's movable half; **the hand-copied catalog collapsed to one** —
      `exec_tool_catalog()` is now eight lines mapping `tddy_tool_engine::tool_catalog()` into the
      MCP shape, and the advertised set is byte-identical over the real `--mcp` wire ✅
- [x] **`tddy-terminal-rpc`**: `pty_relay`'s four dispatch modes; the clap struct stays behind ✅
- [~] **`tddy-core`**: `toolcall_client`, `cli.rs`'s wire types and relay pairs, and `action_tools`'
      manifest rules. **Four of the six movers stay in `tddy-tools`** — see `## Boundaries`
- [x] **`tddy-bsp`**: `build_cli` dispatch; **the duplicate `plugin_registry` deleted**; the 6 `tddy-build*` deps dropped from `tddy-tools` ✅
- [x] **`tddy-code-analysis`, `tddy-code-restructuring`, `tddy-lsp-executor`**: their dispatches; `cli_vector()` deleted ✅
- [ ] **`tddy-tools`' `build.rs` deleted**; the cross-package `include_dir!`/`include_str!` reach gone
- [ ] **Dependency drops asserted**: `tddy-daemon`, `tddy-sandbox-app`, `tddy-sandbox-darwin` no longer dev-depend on `tddy-tools`
- [ ] **Env contract**: all 25 `TDDY_*` variables read from the same places, verified. **One added at
      M5**: `TDDY_SESSION_ACTION_TOOLS`, the host's claim to serve the action surface — none renamed
- [ ] **MCP surface**: 43 advertised over the real `--mcp` stdio wire on a transport that serves the
      action tools; **40 on the daemon path**, the three withdrawn ones being the blocking-todo fix
- [ ] **File budget**: record which over-500-line files landed under budget and which did not, with why
- [ ] **Baseline**: `./test` per touched package back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

`tddy-tools`: 28 files, 12,354 raw / **10,467 non-blank** prod LoC, ~1,900 of it inline `#[cfg(test)]`;
50 test files / 12,158 LoC; 17 workspace + 17 external dependencies; a `build.rs`; a second `[[bin]]`
(`execute-tool-stdio-fixture`) whose deps are forced into `[dependencies]` because plain `cargo build`
builds it.

`server.rs`'s six seams: **A** permission engine (~250, zero tddy deps), **B** PR-stack tools
(~1,000), **C** dynamic tool proxy (~190), **D** subagent runtime (~1,375), **E** `ServerHandler`
wiring (~230), **F** inline tests (1,089 raw).

Three import cycles: `action_tools → server`, `lsp_tools → server`,
`session_agents/{seed,stream} → server`.

Three process-global singletons: `subagent_sessions()`, `session_agent_roster()`,
`livekit_room_cache()`.

### State B

`tddy-tools`: `main.rs`, `lib.rs`, the clap arg structs, seam A and seam E. No `build.rs`. 17 fewer
dependencies. Zero import cycles. The 43 advertised MCP tools are assembled from the crates that own
them. `tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin` do not depend on it at all.

### Delta

#### tddy-tools
- **Architecture**: 24 modules leave; `lib.rs` drops to the remaining surface; `build.rs` deleted
- **Dependencies**: the 6 `tddy-build*`, plus `include_dir`, `jsonschema`, `reqwest`, `prost`, `bytes`,
  `uuid` and the LiveKit optionals, drop as their users leave

#### tddy-workflow-recipes
- **API**: gains the PR-stack MCP tool surface, the GitHub PR REST client, and schema validation —
  which reads the `goals.json` and `generated/` it already owns, without a cross-package `include_dir!`

#### tddy-service
- **API**: gains `SessionToolTransport` and the roster/conversation client. **Node 7 compiles against this**

#### tddy-tool-engine
- **API**: gains `dynamic_proxy::is_native_tool_denied_in_remote_mode`; **one catalog instead of
  two** — `tddy-tools` maps `tool_catalog()` into the MCP shape at one place, as it already does
  for `tddy_lsp_executor`'s. **Node 8 compiles against this**
- **Dependencies**: unchanged. No `rmcp`, no `anyhow`

#### tddy-terminal-rpc
- **API**: gains `pty_relay::{PtyRelayConfig, run_pty_relay}` — four dispatch modes — and an
  internal `local_terminal` that both relays share instead of a `RawMode` each.
  **Node 6 compiles against this**
- **Dependencies**: `+tddy-service` and `+reqwest` (the three daemon-facing modes), and
  `+tddy-livekit` behind a new **non-default** `livekit` feature. No `clap`

#### tddy-bsp
- **API**: gains the build dispatch; **`plugin_registry` exists once**; `run_build`/`run_build_list`
  take no relay function pointer — `RelayFuture` and `ToolcallRelay` are deleted

#### tddy-core
- **API**: gains `toolcall::dispatch_toolcall` (the client half of the wire whose listener it already
  hosts), the CLI's request/response shapes beside their `*RequestWire` counterparts,
  `session_actions::authoring` (the manifest rules `action_tools` was carrying) and
  `session_actions::{SESSION_ACTION_TOOLS_ENV, session_action_tools_enabled}`
- **Duplications**: `cli.rs`'s `QuestionOption` was a field-for-field copy of
  `tddy_core::backend::QuestionOption` — the moved `AskQuestionItem` re-exports the original rather
  than declaring a second one, and the bytes on the wire are identical either way. A **fifth**
  duplication is left standing and recorded below: `MAX_MANIFEST_BYTES` is `64 * 1024` in both
  `tddy_core::session_actions::authoring` and `tddy_sandbox_app::host_actions`

## Implementation Milestones

- [x] M1 — the `tddy-testing-commons` cycle resolved ✅
- [x] M2 — `mcp_primitives` extracted; all three `server.rs` cycles broken; `cargo build -p tddy-tools` clean ✅
- [x] M3 — the four small dispatch movers (`analyze_cli`, `restructure_cli`, `lsp_tools`, `build_cli`); `cli_vector()` and the duplicate `plugin_registry` deleted ✅
- [x] M4 — `schema`, `schema_manifest`, `github_pr` to `tddy-workflow-recipes`; `review_persist` deleted; `build.rs` deleted ✅ (seam B stays — see `## Boundaries`)
- [~] M5 — `toolcall_client`, `cli.rs`'s wire types/relay pairs and `action_tools`' manifest rules to
      `tddy-core`; the action-tool advertisement withdrawn; `tddy-bsp`'s `ToolcallRelay` parameter gone.
      **`session_hook`, `list_models`, `session_actions_cli`, `session_context` do not move** (`## Boundaries`)
- [x] M6 — `pty_relay` to `tddy-terminal-rpc` (780 non-blank prod lines → **142** here, the doc,
      the clap struct and the hand-off; **698** there, plus a 55-line `local_terminal` that ends a
      fourth duplication);
      seam C's catalog collapsed to one; `is_native_tool_denied_in_remote_mode` to
      `tddy_tool_engine::dynamic_proxy`. **`build_dynamic_tool_list`, `dynamic_tool_router` and
      `static_tool_names` stay in `tddy-tools`** (`## Boundaries`) ✅
- [ ] M7 — `session_tool_client` + `session_agents/*` to `tddy-service`; the three dev-deps dropped and asserted
- [ ] M8 — seam D + `registry.rs` + `relay.rs` to `tddy-discovery`
- [ ] M9 — the 43-tool MCP surface verified over the real stdio wire; env contract verified; baselines restored

## Testing Plan

**Primary test level: integration, per package.** 12,158 LoC of dedicated suites already exist, and
several of the heaviest — `subagent_async_response_acceptance.rs` (671),
`request_action_mcp_acceptance.rs` (569), `subagent_mcp_acceptance.rs` (374),
`mcp_stdio_dynamic_tools_acceptance.rs` (196) — drive `tddy-tools` **through its real `--mcp` stdio
wire** with `assert_cmd`. Those are the most valuable tests in this node: they exercise the binary
from outside, so they prove the MCP surface is unchanged **regardless of which crate now implements
it**, and they move with the seam rather than being rewritten.

Four proofs beyond the moved suites:

- **`restructure verify --against <pre-move ref>`** from the repo root.
- **The moved-line diff**, alongside the visibility table, never instead of it.
- **Three dependency-drop assertions** — `tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin`
  must not have `tddy-tools` on any dependency path, dev included. This is the node's headline outcome
  and it is asserted, not described.
- **A 43-tool advertisement audit** — enumerate the advertised tool names over the real stdio wire
  before and after, and require the sets identical. With the tools now assembled from six crates, an
  omission would otherwise show up only as a missing capability at an agent's runtime.

The env contract gets its own check: the 25 `TDDY_*` variables are read from the same modules
afterwards, verified by grep over the destinations rather than by assumption.

## Acceptance Tests

### tddy-tools
- [ ] **Integration**: `--mcp` advertises exactly the same 43 tool names as before the move (`mcp_tool_advertisement_audit.rs`)
- [ ] **Integration**: every CLI subcommand still dispatches (`cli_integration.rs`)
- [ ] **Integration**: `approval_prompt` decides and relays undecidable cases, still in this crate (`permission_engine_acceptance.rs`)
- [ ] **Unit**: `tddy-tools` has no import cycle — `mcp_primitives` is reached by all three former cycle participants (`mcp_primitives.rs`)

### tddy-service
- [ ] **Integration**: `dispatch_session_tool` reaches a daemon over all four transports from the new crate (`session_tool_dispatch_acceptance.rs`)
- [ ] **Integration**: the roster subscription reconnects with the documented backoff (`session_agent_roster_client_acceptance.rs`)

### tddy-tool-engine
- [ ] **Integration**: the dynamic tool proxy forwards to a daemon (`dynamic_tool_router_acceptance.rs`)
- [x] **Unit**: there is exactly **one** exec-tool catalog; `tddy-tools`'
      `exec_tool_catalog_names_match_workspace_exec_tool_names` is **deleted** — with the copy gone
      it asserted exactly what `tddy_daemon::tool_catalog_sync`'s
      `workspace_exec_tool_names_match_tool_catalog` already asserts. **The daemon's is kept**: it
      guards a pair that has *not* collapsed, this catalog against `tddy_sandbox::
      workspace_exec_tool_names` (the `--allowedTools` a sandboxed `claude` is spawned with) ✅

### tddy-workflow-recipes
- [ ] **Integration**: the 14 `pr_*` MCP tools answer from the new crate (`pr_stack_tool_dispatch_acceptance.rs`)
- [ ] **Integration**: schema validation resolves `goals.json` without a cross-package `include_dir!` (`schema_validation_tests.rs`)

### tddy-terminal-rpc
- [x] **Unit**: the relay builds a `StartSession` from its config and speaks the daemon's OSC
      resize format (`pty_relay.rs`). The four modes' *connection* behaviour is still only covered
      by the mode-selection branch in `run_pty_relay`; a `pty_relay_acceptance.rs` against a fake
      daemon would be new coverage, not moved coverage, and is left to node 6 with the server it
      needs

### tddy-discovery
- [ ] **Integration**: a subagent conversation opens, prompts, awaits and cancels (`subagent_async_response_acceptance.rs`)

### tddy-bsp
- [ ] **Integration**: `build` and `build-list` dispatch, with one `plugin_registry` (`build_cli_acceptance.rs`, `demo_build_plugin_acceptance.rs`)

### tddy-daemon / tddy-sandbox-app / tddy-sandbox-darwin
- [ ] **Integration**: `tddy-tools` is absent from each crate's dependency graph, dev-dependencies included (`dependency_boundary_unit.rs` in each)

## Decisions & Trade-offs

- **Seams A and E stay in `tddy-tools`.** The permission engine and the `ServerHandler` router
  assembly *are* the MCP server, which is the crate's reason to exist. Discovery noted seam A is cheap
  to move (zero tddy dependencies), but it carries a bespoke NDJSON Unix-socket protocol that
  `toolcall_client` deliberately migrated away from — relocating it would spread that protocol into a
  library rather than retire it. The endpoint is a `tddy-tools` of roughly 700 lines that parses
  arguments, decides permissions and assembles a router. That is a coherent crate, not a husk.
- **Seam B stays too, and that is a change from the plan.** Discovery counted seam B at ~1,000 LoC
  and read it as PR-stack logic sitting in the wrong crate. It is not: every one of the 16 bodies is
  an adapter over a `tddy_workflow_recipes::pr_stack` function that already exists, so the only
  thing available to move is the `#[tool]` advertisement and the `Pr*Input` schemas — ~415 LoC of
  MCP surface. Moving those would put `rmcp` and `schemars` into `tddy-workflow-recipes`, a crate
  that today has no MCP dependency at all, and push that build cost onto everything that depends on
  it. Weighed against a scope line, the dependency edge is the more expensive of the two, and the
  node's own rule for seams A and E already says which way to resolve it.
- **The action tools are withdrawn rather than implemented, and the gate is a claim, not the
  transport.** The blocking todo offered two ways out: implement `EstablishAction`/`ListActions`/
  `InvokeAction` on the daemon path, or gate the merge on a transport that serves them. The first is
  three new daemon round-trips — a feature, in a node that is a behaviour-preserving restructure.
  The second turned out not to be expressible as written: `SessionToolTransport::SandboxIpc` is the
  transport for `tddy-sandbox-app`'s `AppToolHandler`, which *does* implement all three, **and** for
  `tddy-daemon`'s `DaemonToolHandler`, which implements none of them, and the in-jail server cannot
  tell the two apart from the socket. So the host says so, with `TDDY_SESSION_ACTION_TOOLS` — the
  same shape as the `TDDY_LSP_TOOLS` gate one line below it in the same router, and the same shape
  as `TDDY_SUBAGENT_ROSTER_STATIC`. The gate constant lives in `tddy_core::session_actions`, the
  crate that owns actions and the only one both the advertiser (`tddy-tools`) and the implementer
  (`tddy-sandbox-app`) already name.
  **This changes the advertised set on the daemon transport, and M9's audit must say so rather than
  reconcile it**: measured over the real `--mcp` stdio wire, a fully configured session advertises
  **43** with the claim and **40** without, differing in exactly `request_action`, `list_actions`,
  `invoke_action` and nothing else. The three that go are the three that answered
  `{"error":"unknown tool: ListActions","is_error":true}` to every call.
- **`anyhow` was added to `tddy-core`, on an explicit developer decision.** The crate was
  deliberately `thiserror`-only — zero `anyhow` and zero `clap` hits across its whole `src` tree —
  and three of M5's movers were blocked on that alone. The two ways out were to add the dependency
  or to rewrite each moved body into `tddy-core`'s error style. The rewrite was rejected: it breaks
  the verbatim-move property this node's behaviour-preserving claim rests on, turning a restructure
  into a rewrite of four modules' error handling. `anyhow` is one widely-used crate already
  everywhere else in the workspace, so it adds no new third-party code to the tree — only a new
  edge from the base library. It unblocked `session_context` outright and the non-CLI halves of
  `list_models` and `session_actions_cli`; it did **not** unblock `session_hook`, which is a cycle.
- **`tddy-terminal-rpc` gained `reqwest` and `tddy-service`, and that is the price of the
  `pty_relay` move.** Three of the four dispatch modes talk to the daemon: two POST
  connect-protocol frames at `connection.ConnectionService` and `auth.AuthService` over HTTP, and
  one reads an envelope-framed streaming response. The protos are `tddy-service`'s and the client
  is `reqwest`; there is no version of the move that leaves either behind. The alternative was to
  rewrite the transport onto `tddy-rpc`, which the crate already has — rejected for the same
  reason M5 rejected rewriting error handling into `tddy-core`'s style: it turns a
  behaviour-preserving move into a rewrite of the thing being moved, in the node whose whole claim
  is that nothing changed. The cost is bounded and measured: `tddy-terminal-rpc`'s only
  reverse-dependencies are `tddy-daemon`, `tddy-coder` and `tddy-tools`, and **all three already
  depend on `reqwest` and `tddy-service` directly**, so the workspace builds no crate it did not
  build before. `tddy-livekit` came with the LiveKit mode as an **optional** dependency behind a
  new `livekit` feature, off by default and enabled through `tddy-tools`' own — so a
  `--no-default-features` in-jail build still carries no SDK. Verified by building `tddy-tools`
  both ways.
- **A fourth duplication is deleted, found by the move rather than by the plan.**
  `pty_relay.rs` and `tddy-terminal-rpc`'s `local_pty_relay.rs` each declared a byte-identical
  `RawMode` termios guard and terminal-size probe — the same `24 × 220` fallback, the same
  `cfmakeraw`. Two of them in one crate would have been absurd, so they became
  `tddy_terminal_rpc::local_terminal`, used by both relays. This is the pattern the other three
  duplications followed: each existed *because* one copy was locked in a binary crate.
- **Breaking the three cycles is step zero, in the same PR.** It is ~8 items in one new module and it
  gates every other move here. Making it a separate node would produce a PR whose only deliverable is
  a module nobody imports yet — the stubs-only shape the boundary contract forbids.
- **Eleven destinations in one PR.** This is the largest fan-out in the stack and the strongest
  argument for the 25-node plan. It is one node because the eleven moves are not independent: they all
  wait on the cycle break, they share `server.rs` as a source, and three of them (`tddy-service`,
  `tddy-tool-engine`, `tddy-terminal-rpc`) must land together because nodes 6–8 compile against all
  three. Splitting them would serialise nodes 6–8 behind three separate PRs instead of one.
- **The three duplications are deleted here, not recorded.** Each exists *because* its original was
  locked in a binary crate, so the move is the only moment at which deleting it is safe and obvious.
  `plugin_registry` and the exec-tool catalog both have matched guard tests naming the duplication —
  the codebase already knows about them and is paying to keep them in step.
- **The env contract is treated as an interface.** 25 `TDDY_*` variables, `TDDY_SOCKET` read at 43
  sites, are how in-jail agents reach their host. They are not renamed, and the audit that they are
  read from the same places is a testing-plan item rather than an assumption.

## Technical Debt & Production Readiness

- [x] **M3 left `tddy-bsp`'s build dispatch taking its relay transport as a function pointer.**
      Discharged at **M5**: `toolcall_client` is `tddy_core::toolcall::client`, `build_cli`'s
      `run_build`/`run_build_list` take no `dispatch` argument, `RelayFuture` and `ToolcallRelay` are
      deleted, `main.rs`'s `relay_toolcall` adapter is gone and so is the `TODO(tools-thinning)` ✅
- [ ] **`MAX_MANIFEST_BYTES` is still declared twice.** M5 gave it a home in
      `tddy_core::session_actions::authoring`; `tddy_sandbox_app::host_actions` still declares its own
      `64 * 1024`, which is the authoritative host-side bound on the same value. Collapsing it is one
      line, deliberately not taken in M5 because `tddy-sandbox-app` is outside the milestone's
      verified package set
- [ ] Seam A's NDJSON Unix-socket protocol survives; retiring it in favour of `tddy-rpc` framing is a
      `docs/dev/todo/` entry at wrap
- [ ] `tddy-service` depends on `tddy-tui`, so moving `session_tool_client` there pulls the TUI into
      every consumer's build — including in-jail binaries. Recorded; a `tddy-session-tool-client` crate
      is the alternative if the build cost proves real
- [ ] **M6 gave `tddy-terminal-rpc` the same edge, for the same reason.** `pty_relay` encodes
      `connection.ConnectionService` and `auth.AuthService` messages, so the crate now depends on
      `tddy-service` and therefore transitively on `tddy-tui`. Today this costs nothing measurable —
      all three of its reverse-dependencies already depend on `tddy-service` directly — but it is the
      second crate to acquire the edge, and both would be fixed by the same split of the protos out
      of `tddy-service`
- [ ] The second `[[bin]]` (`execute-tool-stdio-fixture`) still forces `tddy-rpc`, `tddy-stdio` and
      `async-trait` into `[dependencies]` rather than dev-deps
- [ ] **Step zero traded three cycles for one, and M7 inherits it.** `open_roster_agent_session`
      resolves against the live roster, so `mcp_primitives` imports `session_agents` while
      `session_agents/{seed,stream}` import `seed_subagents_or_report` and `env_non_empty` back out
      of it. In-crate that compiles; at **M7**, with `session_agents/*` in `tddy-service`, it is the
      cross-crate cycle step zero exists to prevent. M7 re-homes those two items (the roster seed
      belongs with the roster) rather than moving the modules as they stand
- [x] `lsp_tools`' two `crate::server::PermissionServer::new().tool_names()` calls are `#[cfg(test)]`
      only, and cannot follow `lsp_tools` to `tddy-lsp-executor` at **M3** — the advertisement they
      assert is a `tddy-tools` fact. They re-home to a `tddy-tools` integration test, not to the
      destination crate ✅ `packages/tddy-tools/tests/lsp_tool_advertisement_acceptance.rs`

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-tools` | 80 lib tests passing before `mcp_primitives` was added | |
| `cargo clippy -p tddy-tools -p tddy-service -p tddy-tool-engine -p tddy-terminal-rpc -p tddy-discovery --all-targets -- -D warnings` | ✅ exit 0 | |
| advertised MCP tool count over `--mcp` | **43** | |

**Per-milestone package baselines.** Every drop is accounted for by a named move; a package whose
number falls without one is a regression, not a milestone.

| Package | Pre-M5 | Post-M5 | Accounted for |
|---|---:|---:|---|
| `tddy-tools` | 349 | **337** | −9 `action_tools` manifest tests, −4 moved relay-acceptance files, +1 new advertisement-gate test |
| `tddy-core` | 563 | **583** | +9 manifest tests, +1 for the now-public `author_prompt`, +4 moved relay-acceptance files, +3 `client_wire`, +3 `tool_gate` |
| `tddy-bsp` | 12 | **12** | unchanged — the `ToolcallRelay` parameter had no test of its own |

| Package | Pre-M6 | Post-M6 | Accounted for |
|---|---:|---:|---|
| `tddy-tools` | 334 | **331** | −3 `pty_relay` tests moved, −1 `is_native_tool_denied_in_remote_mode` moved, −1 vacuous guard test deleted, +2 new arg-hand-off tests |
| `tddy-terminal-rpc` | 23 (+1 failing) | **24** / 25 with `--features livekit` | −2 fabricated stub tests deleted, +3 moved from `pty_relay` (one of them LiveKit-gated) |
| `tddy-tool-engine` | 9 (+2 failing) | **11** | −2 fabricated stub tests deleted, +2 for the moved remote-mode denial |

**16 failing tests** define this node: 5 in `tddy-tools`' `mcp_primitives` (step zero — the module
that breaks all three `server.rs` cycles), 1 in `tddy-service`'s `session_tool_client`, 2 in
`tddy-tool-engine`'s `dynamic_proxy`, 1 in `tddy-terminal-rpc`'s `pty_relay`, 2 in
`tddy-discovery`'s `roster`, and **3 dependency-drop assertions** — one each in `tddy-daemon`,
`tddy-sandbox-app` and `tddy-sandbox-darwin`.

**Three of those sixteen were deleted at M6 rather than made to pass**, because they asserted
designs the code does not have. `dynamic_proxy`'s `stops_advertising_a_tool_an_agent_has_taken_over`
required advertisement to be filtered by the roster; it is not, and must not be — `--allowedTools`
is fixed when `claude` spawns, so a takeover can only be enforced at dispatch. Its sibling
`advertises_this_crates_own_catalog_when_the_host_adds_nothing` and `pty_relay`'s two both called
signatures that never existed (`build_dynamic_tool_list(host_tools, withdrawn)`,
`run_pty_relay(RelayMode) -> Result<(), RelayError>`). They are replaced by four tests over the
surfaces that do exist. **Nodes 6 and 8 compile against the corrected shapes**, listed in
`## Draft PR contract`. Those three are asserted against the manifest rather
than described, because a **dev**-dependency survives invisibly: nothing fails to compile when it is
merely unused.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-tools-thinning.md` — the release-note file, carrying the
      three dependency drops, the three deleted duplications and the before/after module counts
- [ ] Move `packages/tddy-tools/docs/json-schema.md` to `tddy-workflow-recipes/docs/`
- [ ] Close `docs/dev/todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md`
- [ ] New `docs/dev/todo/` entries: retire seam A's NDJSON protocol; `tddy-service`'s `tddy-tui`
      dependency; the forced `[[bin]]` dependencies
- [ ] Doc triage: `grep -rn -e 'tddy_tools' -e 'tddy-tools' packages/*/README.md packages/*/docs docs/ft`
