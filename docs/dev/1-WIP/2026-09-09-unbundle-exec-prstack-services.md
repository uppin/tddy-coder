# Changeset: the catalogue, exec-tool and PR-stack services

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **8 of 8** — the last. PR [#477](https://github.com/uppin/tddy-coder/pull/477).
Base: `feature/unbundle/session-agent-services` (node 7, PR #476)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-unbundle-exec-prstack-services-initial-discovery.md](./2026-09-09-unbundle-exec-prstack-services-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

The last 16 methods this stack moves leave `connection.ConnectionService`, taking it from 33 to **17**.

| New coordinate | Family | Methods | Served by | Source that moves |
|---|---|---:|---|---|
| `catalog.CatalogService` | A | 4 | `tddy-discovery` | `agent_list_mapping.rs` (94) |
| `exec_tools.ExecToolService` | L | 4 | `tddy-tool-engine` | `tool_call_log.rs` (276), `session_toolcall.rs` (218) |
| `pr_stack.PrStackService` | P | 8 | **`tddy-daemon`** (`pr_stack_rpc.rs`) | the PR-stack handlers — not `tddy-workflow-recipes` (adding `tddy-service` there creates a dependency cycle) |

**This node adds no new crates.** All three services are served from crates that already own their
domains after node 5. It is the node that finishes the job rather than one that creates more structure.

### The local socket keeps its surface

Per the policy node 7 set: every family that was reachable on the local Unix socket as part of
`connection.ConnectionService` **stays reachable there after it moves**. Dropping one is a silent
capability removal on a privileged interface, and the failure mode is a caller that used to work
receiving `unimplemented` with no announcement.

So all three of this node's services go on the socket: `catalog.CatalogService` (4),
`exec_tools.ExecToolService` (4) and `pr_stack.PrStackService` (8) = **16 adapter methods**,
**generated** by node 6's `generate_tonic_adapter`.

`exec_tools.ExecToolService` matters most of the three: `ExecuteTool` is the method
`tddy-sandbox-runner`'s relay allowlist gates at `runner.rs:69` and `tddy-sandbox-app`'s mirror guard
names — so it is reached from inside a jail, and a family that silently left the socket would fail
there rather than anywhere a developer is looking.

Three things end here:

1. **The exec-tool catalog duplication closes completely.** `tddy-tool-engine` already defines and
   executes the ten tools; after this node it also *serves* them. The guard tests that existed to keep
   two hand-copied catalogs in step — one in `tddy-tools`, one in `tddy-tool-engine`, plus the daemon's
   `tool_catalog_sync.rs` that node 3 relocated — have nothing left to guard and are deleted.
2. **The sandbox relay allowlist's other half moves.** `runner.rs:69`'s
   `service == "connection.ConnectionService" && method == "ExecuteTool"` and its mirror guard at
   `tddy-sandbox-app/src/sandboxed_session.rs:708` are both family L.
3. **The last hand-built URL goes.** `tddy-discovery/src/tools.rs:141` composes
   `format!("{}/connection.ConnectionService/ExecuteTool", …)` with four wiremock path assertions
   behind it — the only place in the repo where a `ConnectionService` coordinate is a string literal
   in a URL rather than a generated client call, and therefore the one consumer a proto change cannot
   break at compile time.

## Boundaries

This PR explicitly does **not**:

- Move families C, D, O or Q. **Those 17 methods are the deliberate endpoint**: a daemon that starts,
  resumes, signals and deletes sessions, owns projects and their branches, runs the demo VM, and mints
  a local token over a peer-credentialled socket. Taking them would leave nothing coherent behind.
- Move `session_list_enrichment.rs`, `split_session.rs`, `cli_session_manager.rs`,
  `workspace_session.rs`, `session_deletion.rs`, `session_reader.rs`, `project_storage.rs` or
  `project_provision.rs`. All belong to families C and D and stay.
- **Hand-write a tonic adapter.** A gap in node 6's generator is reported upward, not worked around
  with 16 `async fn`s — hand-writing them is the cost the generator exists to remove.
- Change what the sandbox relay allowlist *permits*. Identical operation set; only the service name
  each condition carries changes.
- Widen `tddy-discovery`'s dependencies to take a `DaemonConfig`. `main.rs` already extracts rows with
  `agent_list_mapping::agent_allowlist_rows(&config, &[])`, so the crate takes rows.
- Create a `tddy-pr-stack-service` crate. `tddy-workflow-recipes` already owns `orchestrate_pr_stack`,
  `pr_stack`, `plan_pr_stack` and — since node 5 — the 14 PR-stack MCP tools. A separate crate would
  split one domain across two.
- Force every file under 500 lines. The remaining over-budget files are in families C and D and stay
  with the daemon; what this node touches is already under budget or brought under it by its seams.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate`; `types.proto`; the proto-split pattern | all three protos import `types.proto` for `ToolDef`, `ExecuteToolRequest`/`Response`/`Chunk`, `BranchResolution` and the branch views | change `types.proto`'s shape |
| **`n5` tools-thinning** | **the single exec-tool catalog and the dynamic tool proxy in `tddy-tool-engine`; the PR-stack MCP tools and `github_pr` in `tddy-workflow-recipes`; the subagent roster in `tddy-discovery`** | each of the three services is served from the crate node 5 made the owner of its domain. Its `Draft PR contract` fixed `tddy-tool-engine`'s surface for exactly this | re-introduce a second catalog, or move any of those surfaces again |
| `n3` sandbox-spawn-services | the relocated `tool_catalog_sync.rs` guard test | this PR deletes it, because with one catalog it compares a thing to itself | delete it before node 5 has collapsed the catalog — the guard is what proves the collapse was correct |
| `n7` session-agent-services | the family-B tuples in `packages/tddy-sandbox-runner/src/runner.rs:89-94` | this PR edits the **same file** at `:69` for family L | change node 7's family-B tuples |
| **`n6` session-io-services** | **a working `generate_tonic_adapter`** in `tddy-codegen` | all three of this node's services go on the local socket per the policy, and their 16 adapter methods are generated. A dependency on node 6's **behaviour**, not its published surface — which is what puts this node in wave 4 | implement or extend the generator, or hand-write an adapter; a gap is reported to node 6 |
| `n2`, `n4` | nothing this PR consumes | — | — |

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-service/proto/catalog.proto` (4 rpcs), `exec_tools.proto` (4) and `pr_stack.proto`
  (8), all importing `types.proto`.
- The three `build_*_entry(...) -> ServiceEntry` signatures in `tddy-discovery`, `tddy-tool-engine`
  and `tddy-workflow-recipes`, bodies annotated `// TODO(exec-prstack-services): implement`.
- The updated conditions in `packages/tddy-sandbox-runner/src/runner.rs:69` and
  `tddy-sandbox-app/src/sandboxed_session.rs:708` — **pushed early because it is the second
  security-relevant edit in the stack**, for the same reason node 7 pushed the first.
- The failing acceptance tests, including the through-a-real-jail tool-execution test and the
  final-shape assertion that `connection.ConnectionService` declares exactly 17 methods.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** **4 of 5** — moved from 3 by the local-socket policy.
**Greenable independently:** **not until node 6 is green.** Two reasons. All three services are served
from crates node 5 makes the owners of their domains, and `tddy-tool-engine`'s surface was fixed by
node 5's draft contract for this node to compile against — that was always true. And under the
reachability policy this node's 16 adapter methods are **generated** by node 6's
`generate_tonic_adapter`, which is a dependency on a predecessor's *behaviour*: the generator has to
actually work.
**Concurrent with:** node 7, which depends on node 6 for the same reason and shares this wave.
**Blocks:** node 9, which is wave 5. This node is no longer the top of the stack.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8      n6 → n7, n8      n7, n8 → n9

    w1  n1
    w2  n2, n3, n5
    w3  n4, n6
    w4  n7, n8        ← this node
    w5  n9

The local-socket policy is what deepened the graph from four waves to five.

⚠ **Three recurring conflicts**: `packages/tddy-daemon/src/runtime.rs` (every node),
`packages/tddy-coder/src/session_participant/mod.rs` (nodes 6, 7, 8), and
`packages/tddy-sandbox-runner/src/runner.rs` — node 7 owns `:89-94`, this node owns `:69`. Keep every
node's changes.

## Affected Packages

- **tddy-discovery** — serves `catalog.CatalogService`; gains `agent_list_mapping.rs`; its hand-built
  URL becomes a generated client call
- **tddy-tool-engine** — serves `exec_tools.ExecToolService`; gains `tool_call_log.rs` and
  `session_toolcall.rs`; **defines, executes and serves the same ten tools**
- **tddy-daemon** — serves `pr_stack.PrStackService` via `pr_stack_rpc.rs` (handler ports on `ConnectionServiceImpl`; recipes crate keeps orchestration only)
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — 3 modules and 588 prod LoC leave;
  `connection.ConnectionService` reaches its final 17 methods
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — the 931-line
    endpoint reference reaches its final shape
- **tddy-sandbox-runner**, **tddy-sandbox-app** — the family-L allowlist and its mirror guard
- **tddy-coder**: `src/session_participant/mod.rs` — family L **in lockstep**
- **tddy-service**: three protos appear; `connection.proto` reaches its final shape
- **tddy-web**: `SessionInspectorDrawer`, `PrStackScreen`, `useQueryBranch`, `useAvailableAgents`,
  `useSelectableAgents`, `useAgentModels`, `CreateSessionPane`

## Related Feature Documentation

- [PRD-2026-09-09-exec-prstack-services.md](../../ft/daemon/1-WIP/PRD-2026-09-09-exec-prstack-services.md)

## Arithmetic, corrected in wave 2

This document and its PRD said the residual was **21 methods**. Counting the declarations in
`connection.proto` says **17**, and node 1's changeset had it right all along:

| Node | Families | Methods |
|---|---|---:|
| 1 | E, F, G, H | 17 |
| 4 | T | 1 |
| 6 | I, J, K, R, S | 22 |
| 7 | B, M, N | 17 |
| 8 | A, L, P | 16 |
| **moved** | | **73** |
| **residual** | C (8), D (5), O (3), Q (1) | **17** |

The 21 came from an arithmetic slip while consolidating 25 nodes into 8, and it survived into two
documents because nothing checked it. Node 8's completion test now enumerates the residual **by
name** rather than by count, so a method forgotten by every node — or added while the stack was in
flight — fails there instead of quietly surviving.

## Summary

The tools-and-agents catalogue, tool execution and PR-stack orchestration leave
`connection.ConnectionService` for three services hosted by the crates that already own those domains.
No new crates. `connection.ConnectionService` ends at 17 methods.

## Background

See the PRD. The framing worth keeping in view while reviewing: **this node's value is subtraction.**
It creates nothing new — no crate, no abstraction — and its deliverables are a duplication finally
closed, a hand-built URL replaced by a generated call, two guard tests made unnecessary, and a proto
that has stopped being a god-schema.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md` (verify)

Node 5 was scoped to resolve this by implementing the action tools where they are advertised or
withdrawing the advertisement. This node serves the exec-tool surface those advertisements sit beside,
so it is where a partial resolution would show up as a live `Unimplemented` on a served coordinate.
Verify the entry is closed before this node's own advertisement audit passes. Earns a `## Scope` line.

### ⛔ BLOCKING — the `tddy-coder` lockstep requirement

Family L (`ExecuteTool`, `ListExecTools`, `ListSessionToolCalls`) is served by `tddy-coder`'s session
participant as well as by the daemon. Same argument as nodes 6 and 7, and the last time it applies.
Earns a `## Scope` line.

### ⚠ DURING — `2026-08-13-tddy-daemon-generalize-pr-stack-spawn-args-to-all-optional-spawn-flags.md`

In family P's path. The move must not make the generalisation harder: the planned-PR mutation
requests keep their optional-field shape rather than being flattened on the way out. Recorded, not
fixed here.

### ⚠ DURING — `2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`, `2026-07-30-pr-stack-full-control-follow-ups.md`, `2026-08-13-pr-stack-*` (3 entries), `2026-08-30-pr-stack-querybranch-s-local-only-legs.md`

Seven open items in the PR-stack surface being moved, `QueryBranch`'s local-only legs among them. All
survive the move unchanged; recorded so a reviewer seeing them in `tddy-workflow-recipes` knows they
are inherited. After this node the whole PR-stack domain — recipes, MCP tools and RPC service — is in
one crate, which is the first point at which several of them are fixable in one place.

### ℹ ANSWERED — `2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md`

Asks who writes `stack-progress.json`. With family P served from `tddy-workflow-recipes`, the answer
becomes unambiguous — the crate that owns the recipe also owns the RPC. Update the entry at wrap.

## Scope

- [x] **Proto**: `catalog.proto` (4), `exec_tools.proto` (4), `pr_stack.proto` (8); `connection.proto`
      reaches 17 methods with vacated numbers `reserved`
- [x] **`tddy-tool-engine` serves family L**; `tool_call_log.rs` moved; guard tests deleted where catalog is single-sourced
- [x] **`tddy-discovery` serves family A**; `agent_list_mapping.rs` wired; hand-built exec-tool URL replaced by generated client
- [x] **Family P served**: `pr_stack.PrStackService` on daemon (`pr_stack_rpc.rs`), not `tddy-workflow-recipes` (cycle)
- [x] **Local socket**: all three services on `BinaryLocalSocketServices`; adapters generated
- [x] **⛔ Sandbox relay allowlist**: `runner.rs` and `sandboxed_session.rs` name `exec_tools.ExecToolService/ExecuteTool`
- [x] **⛔ `tddy-coder` lockstep**: `session_participant` serves `exec_tools.ExecToolService`; full `two_server_parity` deferred to CI
- [x] **⛔ Action-tool advertisement**: closed by node 5 — `docs/dev/todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md` (Resolved 2026-09-10)
- [x] **Web**: hooks/components/Cypress fakes migrated; `scripts/generated-code.sh check packages/tddy-web` passes
- [x] **Final shape**: `connection.ConnectionService` 17 RPCs; `unbundle_service_split` in `tddy-service`
- [~] **Baseline**: scoped gates green (`tddy-service` 22/22, `tddy-web` unit 1178/1178, clippy on touched Rust pkgs); `./test -p tddy-daemon` fails locally on `in_jail_conversation_acceptance` (relay timeout, macOS sandbox fixture) — CI authority for full daemon suite
- [x] **Code Quality**: `cargo clippy -p tddy-daemon -p tddy-discovery -p tddy-tool-engine -p tddy-coder -p tddy-service -- -D warnings`; `cargo fmt -p tddy-daemon`
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

`connection.ConnectionService` has 33 methods after nodes 1, 4, 6 and 7. Families A (4), L (4) and
P (8) are among them. `tddy-tool-engine` defines and executes the ten tools but does not serve them.
`tddy-discovery` composes an exec-tool URL by hand. `tddy-sandbox-runner:69` and
`tddy-sandbox-app:708` name `connection.ConnectionService` + `ExecuteTool`. `tddy-coder`'s participant
serves family L.

### State B

`connection.ConnectionService` has **17 methods** — families C, D, O and Q. Three new services are
served from `tddy-discovery`, `tddy-tool-engine` and `tddy-workflow-recipes`. The tool catalog has one
definition, one executor and one served coordinate. No hand-built RPC URL exists in the repo.

### Delta

#### tddy-service
- **Proto**: `catalog.proto`, `exec_tools.proto`, `pr_stack.proto` added importing `types.proto`;
  `connection.proto` loses 16 rpcs and reaches its final shape, vacated numbers `reserved`
- **Build**: one prost + one tonic pass per new service; all three added to the descriptor set

#### tddy-tool-engine
- **API**: `build_exec_tool_entry`; gains `tool_call_log` and `session_toolcall`
- **Implementation**: the guard tests comparing two catalogs are deleted

#### tddy-discovery
- **API**: `build_catalog_entry`; gains `agent_list_mapping`
- **Implementation**: `tools.rs` uses a generated client; the 4 wiremock assertions target the new path

#### tddy-workflow-recipes
- **API**: `build_pr_stack_entry`

#### tddy-daemon
- **Architecture**: 3 modules leave; three `ServiceEntry` groups move behind their owners' constructors

#### tddy-sandbox-runner / tddy-sandbox-app / tddy-coder / tddy-web
- **Implementation**: coordinate updates, described above

## Implementation Milestones

- [ ] M1 — all three protos generate; `types.proto` imported rather than duplicated
- [ ] M2 — the sandbox allowlist and mirror guard re-pointed; an in-jail tool call works through a real jail
- [ ] M3 — `tddy-tool-engine` serves family L; the guard tests deleted; one catalog end to end
- [ ] M4 — `tddy-discovery` serves family A; the hand-built URL gone
- [ ] M5 — `tddy-workflow-recipes` serves family P
- [ ] M6 — `tddy-coder`'s participant moved; HTTP and LiveKit answer a session identically
- [ ] M7 — web migrated; `connection.ConnectionService` asserted at 17 methods; baselines restored

## Testing Plan

**Primary test level: integration, per package.** The moved suites carry most of the proof —
`tool_engine_acceptance.rs`, `tool_call_log_acceptance.rs`, the PR-stack RPC suites
(`add_planned_pr_unit_tests`, `link_stack_node_rpc_acceptance.rs`,
`planned_pr_mutation_rpc_acceptance.rs`, `pr_stack_base_session_spawn_acceptance.rs`) and the
catalogue suites (`list_agents_allowlist_acceptance.rs`, `list_subagents_acceptance.rs`) all move with
the code.

**Two tests are new because they assert things no moved test can:**

- **A through-a-real-jail tool execution.** As in node 7, an allowlist that no longer matches the
  served coordinate fails *closed*, so a test inspecting the condition would pass while every in-jail
  tool call was broken. The test spawns a real sandboxed session and executes a tool from inside it.
- **The final-shape assertion.** `connection.ConnectionService` declares exactly 17 methods, and they
  are families C, D, O and Q. This is the stack's completion criterion expressed as a test, and it
  belongs in the last node.

Three further proofs:

- **`restructure verify --against <pre-move ref>`** from the repo root.
- **The moved-line diff**, alongside the visibility table.
- **A hand-built-URL grep**, asserted rather than eyeballed: no source file composes an RPC path from
  a service-name string literal.

`tddy-web` keeps `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

### tddy-tool-engine
- [ ] **Integration**: all 4 `exec_tools.ExecToolService` methods answer on Connect-HTTP (`exec_tool_service_acceptance.rs`)
- [ ] **Integration**: `StreamExecuteTool` chunks identically to the old coordinate (`exec_tool_stream_parity_acceptance.rs`)
- [ ] **Unit**: exactly one tool catalog exists, and the two-catalog guard tests are gone (`catalog.rs`)

### tddy-sandbox-runner
- [ ] **Integration**: an agent **inside a real jail** executes a tool through the updated allowlist (`in_jail_exec_tool_acceptance.rs`)
- [ ] **Unit**: the allowlist permits the same operation set under the new service name (`runner.rs`)

### tddy-discovery
- [ ] **Integration**: all 4 `catalog.CatalogService` methods answer, including a registry assistant winning a name tie (`catalog_service_acceptance.rs`)
- [ ] **Integration**: `tools.rs` reaches the exec-tool coordinate through a generated client (`tools.rs`, 4 wiremock assertions)

### tddy-workflow-recipes
- [ ] **Integration**: all 8 `pr_stack.PrStackService` methods answer (`pr_stack_service_acceptance.rs`)
- [ ] **Integration**: `QueryBranch` and `ResolveStackBase` return the same resolutions as the old coordinate (`pr_stack_parity_acceptance.rs`)

### tddy-tool-engine + tddy-coder
- [ ] **Integration**: the same session answers identically through the daemon and through
      `tddy-coder`'s participant for family L (`two_server_parity_acceptance.rs`)

### tddy-web
- [ ] **Cypress component**: the session inspector lists tools and tool calls at the new coordinate (`SessionInspectorDrawer.cy.tsx`)
- [ ] **Cypress component**: the PR-stack screen adds, repoints and reorders a planned PR (`PrStackScreen.cy.tsx`)

### tddy-daemon (the local socket)
- [ ] **Integration**: all three services answer over the **local Unix socket**, not only over
      Connect-HTTP (`local_socket_reachability_acceptance.rs`)
- [ ] **Integration**: an in-jail `ExecuteTool` reaches the daemon over the socket at the new
      coordinate (`in_jail_exec_tool_acceptance.rs`)
- [ ] **Unit**: no hand-written adapter is added by this node (`local_socket_reachability_acceptance.rs`)

### tddy-service
- [ ] **Unit**: `connection.ConnectionService` declares exactly 17 methods, and they are families C, D, O and Q (`connection_final_shape_unit.rs`)
- [ ] **Unit**: no source file composes an RPC path from a service-name string literal (`no_handbuilt_urls_unit.rs`)

## Decisions & Trade-offs

- **The local socket keeps its surface, so the generator is a hard dependency.** This node adds no
  crate but does add 16 adapter methods to the socket, and they are generated rather than
  hand-written. That is what moved this node out of wave 3 — a cost the eight-node plan never
  accounted for because it never set a policy on socket reachability at all.
- **No new crates.** Family P could have had a `tddy-pr-stack-service`, and family A a
  `tddy-catalog`. Both would split a domain that node 5 had just finished consolidating —
  `tddy-workflow-recipes` owns the PR-stack recipes *and* its MCP tools, `tddy-discovery` owns the
  agent defs *and* the roster. Serving from the owner is the point of the whole stack; adding a crate
  in front of it would be the shape this work exists to remove.
- **Family A is answered from rows, not from `DaemonConfig`.** `tddy-discovery` taking a `DaemonConfig`
  would give a leaf crate the daemon's whole configuration type. `main.rs` already extracts what is
  needed with `agent_list_mapping::agent_allowlist_rows`, so the crate takes rows and the wiring keeps
  owning the config.
- **17 methods stay, and that is the answer rather than a compromise.** Families C, D, O and Q are
  session lifecycle, projects and branches, the demo VM, and a UDS-only local token mint. A daemon
  that does those things and wires everything else is exactly what the brief asked for; a
  `connection.ConnectionService` of zero methods would mean inventing a ninth service for the one
  thing the daemon genuinely is.
- **The guard tests are deleted, not kept as regression cover.** With one catalog they compare a value
  to itself. Keeping them would leave a test whose failure is impossible, which is worse than no test
  because it reads as coverage.
- **The hand-built URL fix is in scope.** It is the only consumer in the repo that a proto change
  cannot break at compile time, so leaving it would mean this node's own breakage could reach
  production silently — the exact failure mode the rest of the stack is arranged to avoid.

## Technical Debt & Production Readiness

- [ ] Seven inherited PR-stack items move unchanged; after this node the whole domain is in one crate,
      which is the first point at which several are fixable together
- [ ] `tddy-coder`'s session participant is still a string dispatch rather than a generated trait impl
- [ ] `connection.proto` carries `reserved` field numbers from all five splitting nodes; permanent by
      design, and it makes the file's history legible at the cost of its readability

## Validation Results (`/pr-wrap` 2026-09-12)

| Step | Result |
|---|---|
| `/pr-stack-rebase` | Verify-and-return — `origin/feature/unbundle/session-agent-services..HEAD` is 4 commits (this PR only); 0 behind parent |
| `/validate-changes` | Scope matches implementation; family P on daemon (`pr_stack_rpc.rs`) documented vs original recipes plan (cycle) |
| `/validate-tests` | Acceptance suites migrated to `TestDaemon` / split clients; fluent-tests preserved in touched Cypress |
| `/validate-prod-ready` | No test-only branches in production paths; `install_self_handle` mirrors `runtime::build` |
| `/analyze-clean-code` | Large-file splits deferred (stack: `rpc_service.rs` shared with dependents) |
| Lint (scoped) | `cargo fmt` + `cargo clippy -p tddy-daemon -p tddy-discovery -p tddy-tool-engine -p tddy-coder -p tddy-service -- -D warnings` clean |
| Tests (scoped) | `tddy-service` 22/22; `tddy-web` `test:unit` 1178/1178; `generated-code.sh check packages/tddy-web` pass |
| Blocker | `in_jail_conversation_acceptance` fails locally (session-agent relay timeout); not marked ready until CI / fix |

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | | local: `in_jail_conversation_acceptance` fail (macOS); rest scoped green in session |
| `./test -p tddy-tool-engine -p tddy-discovery` | | pass (scoped run) |
| `./test -p tddy-coder` | | not re-run this wrap (lockstep on CI) |
| `./dev bun run --filter tddy-web cypress:component` | | spot-checked; full suite on CI |
| `connection.ConnectionService` method count | **33** | **17** |

The known pre-existing failure inherited from node 1's baseline is expected to stay at exactly one.
In-jail suites need a real sandbox backend and are excluded from CI; their local results are stated
rather than claimed from a green build.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-09-unbundle-exec-prstack-services.md` — the release-note file,
      carrying the final 90 → 17 method count for the whole stack
- [ ] `packages/tddy-daemon/docs/connection-service.md` — reduce the 931-line reference to its final
      17 endpoints and point at the eight new services
- [ ] `docs/ft/coder/pr-stacking.md`, `docs/ft/web/pr-stack-live-status.md`,
      `docs/ft/daemon/remote-codebase-mode.md`, `docs/ft/coder/specialized-subagents.md` — new coordinates
- [ ] Update `docs/dev/todo/2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-…md`
- [ ] Verify `2026-08-23-the-action-tools-are-advertised-…md` was closed by node 5
- [ ] Re-read the seven inherited PR-stack entries now that the domain is in one crate
- [ ] **Whole-stack doc pass**: `docs/ft/daemon/rpc-playground.md` and any diagram naming
      `connection.ConnectionService` as the daemon's RPC surface
- [ ] Doc triage: `grep -rn -e 'ExecuteTool' -e 'ListTools' -e 'AddPlannedPr' -e 'QueryBranch' packages/*/README.md packages/*/docs docs/ft`
