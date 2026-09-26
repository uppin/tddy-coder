# Changeset: `tddy-session-lifecycle`'s host methods become receiver-shaped functions over per-topic state and callback ports, in place — the master plan, and its first node: the leaf topics (T7, T8, T10, T11) and the port-free cuts

**Date**: 2026-09-26
**Status**: 📋 Planned. D15 decided (2026-09-26): this plan is cut into five linear conversion nodes (16a–16e) plus one move node (17). Awaiting the developer's review of the remaining open decisions (see "Decisions & trade-offs")
**Type**: Refactor (in-place port restructure; no crate moves; no behaviour change)
**Stack**: `#carve` 16/21, branch `feature/carve/lifecycle-ports` (#531), on top of `#carve` 15 (#526, `feature/carve/lifecycle-split`). **This node is 16a**: it carries the whole stack's master plan (outline, final design, decisions) and delivers M0–M3 only

**Naming.** Nodes are named by their plan label: **16a–16e** are the five in-place conversion nodes
(`#carve` 16–20) and **node 17** is the move node (`#carve` 21, `feature/carve/lifecycle-moves`). The
labels predate the cut (D15), so "node 17" is not `#carve` 17/21, which is 16b.

## Initial Discovery

The stack's discovery is the section [Where every part of tddy-session-lifecycle goes](#where-every-part-of-tddy-session-lifecycle-goes)
below. It was built on 2026-09-26 from the code at #526's tip `9d464a8e`, not from the plan. The
plan's premises were wrong about half the time, and they are corrected in place. The crate-wide
survey behind #524 and #526 is #526's initial-discovery document (not linked: #526 is this node's
parent and is wrapped first, so a link would dangle under the forward-only rule).

## Affected Packages

- **`tddy-session-lifecycle`**: in this node (16a), the port-free cuts (M0), and the four leaf topics
  converted in place: T11 demo VM (`DemoVmState`), T10 presenter (`PresenterObserverDeps`, and the
  `session_notification_publishing` split), T7 admission token (a free function, no state struct) and
  T8 attachments (`AttachmentState`). The host-free leaves (`placement`, `remote_git_pack_execution`,
  `agent_list_mapping`, the new `daemon_urls`) get their imports re-pointed. Public API and facades are
  unchanged.
- **`tddy-session-agents`**: **not touched in 16a.** `AgentRosterState` gains its two fields in 16b.
- **Consumers** (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`): **none is
  edited.** No path changes, because no crate move happens here. (`tddy-desktop` was missing from this
  list before the cut; it names `tddy_session_lifecycle::cli_session_manager::CliSessionManager`.)

The whole plan (16a–16e, then node 17) touches `tddy-session-lifecycle` throughout, widens
`tddy-session-agents`'s `AgentRosterState` in 16b, and creates and fills the receivers only in
node 17.

## Related Feature Documentation

None: this is a behaviour-preserving restructure. There is no PRD.

## Summary

After #526, `tddy-session-lifecycle` holds **20,041 production lines**. Almost everything left
outside the wiring is `impl DaemonSessionHost` methods. They cannot leave the crate (`E0116`), and
the engine can extract only their `self`-free tails: the T3 pilot moved 11 methods for −185 lines.

The plan converts those methods **in place**. Each topic gets:
- a **state value** holding the host fields its bodies read;
- a **callback trait** for the few host capabilities that are not plain fields (a service entry
  list, a peer-routed service, the start path the split re-enters);
- lifecycle's host **builds** the state and **implements** the trait.

After that, every topic module names no `DaemonSessionHost`, no wiring module and no upward topic.
node 17 then moves each topic into its receiver as a plain module move.

**The cut (D15, decided 2026-09-26).** The conversion is ~13k production lines across ~70 files, too
large to review as one PR. It is cut by topic into five linear nodes, each holding the baseline on
its own, then one move node:

| Node | Branch | Stack | Delivers | Lines touched |
|---|---|---|---|---:|
| **16a** (this) | `feature/carve/lifecycle-ports` (#531) | 16/21 | M0–M3: baseline, the six port-free cuts, re-parenting, test relocation, `seeded_clone_guard` split, T11, T10 + the `session_notifications` split, T7 + T8 | ~1.4k |
| 16b | `feature/carve/lifecycle-ports-agents` | 17/21 | M4: T3 agents, `AgentHostCallbacks`, `AgentRosterState` +2 fields | ~2.1k |
| 16c | `feature/carve/lifecycle-ports-split` | 18/21 | M5: T4 + `service_util` + `workspace_session`, `SplitState` / `SplitHost`; CLI imports | ~3.0k (+1.7k CLI imports) |
| 16d | `feature/carve/lifecycle-ports-launch-spawns` | 19/21 | M6 T9 stack spawns + M7a: the jail and CLI-spawn half of T1; `LaunchState` / `LaunchHost` / the owned launch handle | ~4.6k (~2.9k converted) |
| 16e | `feature/carve/lifecycle-ports-launch-start` | 20/21 | M7b: the start/resume half of T1 + M8 T1c coordinate handlers | ~2.5k |
| 17 | `feature/carve/lifecycle-moves` | 21/21 | every converted topic moved into its receiver with the engine; lifecycle wiring-only | ~15.3k moved |

**This node (16a)** delivers M0–M3: after it, T7, T8, T10 and T11 and the host-free leaves name no
`DaemonSessionHost`, no wiring module and no other topic, and the six cross-topic edges that need no
port are cut. T3, T4, T1, T9 and T1c are still host methods.

**What stays reachable:** every public `tddy_session_lifecycle::…` path, and every public
`DaemonSessionHost` method that a consumer or a lifecycle test calls, kept as a thin delegator.
Nothing leaves the crate here, so no facade changes.

## Background

`#carve` shrinks `tddy-session-lifecycle` into a wiring crate:
- #524 (`#carve` 14) destructured it in place;
- #526 (`#carve` 15) moved the host-free leaves out.

#526's port-move pilot showed that the rest cannot be moved by the engine as it stands. Each
host method's head, its calls to other host methods and its `self.clone()` hand-offs keep it bound
to the host. On 2026-09-26 the developer split the remaining work into two parts (later cut into six nodes, D15):
- **the conversion** (16a–16e, `#carve` 16–20; this node is 16a) converts the host methods in place, reviewed as a restructure and
  guarded by the baseline. It is the "purely mechanical … extraction with no behaviour change"
  exception of the PR boundary contract.
- **node 17** moves each converted topic with the engine and carries the wiring target.

The work deferred into these parts:
- the demo VM (`DemoVmState`);
- T10's presenter observation;
- the host-bound remainders of T3, T7 (`svc_resolve_os_user.rs`) and T8;
- T4 (split, behind `SplitHost`);
- T1 with the stack spawns and the coordinate handlers (`LaunchHost`);
- `workspace_session`.

## Responsibility

This node (16a) owns M0–M3:
- **M0**: record the baseline; cut the six wrong-way cross-topic edges that need no port (M0.1–M0.3);
  re-parent the four mixed parent/child files (M0.4, D8); relocate the host-constructing inline test
  (M0.5, D9); split `seeded_clone_guard.rs` (M0.6).
- **M1–M3**: convert T11, T10, T7 and T8 in place into functions over their state value
  (`DemoVmState`, `PresenterObserverDeps`, the admission-token function's two fields,
  `AttachmentState`). None of them needs a callback trait.
- Build each of those states from the host's fields in `handler_state.rs`.
- Make each of those topics, and the host-free leaves, **movable**: no `DaemonSessionHost`, no wiring
  module, no other topic, foundations named by their defining crate (A1–A5 restricted to these
  topics).
- Keep the public API: facades unchanged, and a delegator (in a wiring file) for every public host
  method a consumer or test calls.
- Carry the stack's master plan: the outline, the final design, and every open decision. The
  successor nodes copy what they need from it.

## Boundaries

- **No crate moves.** Nothing leaves `tddy-session-lifecycle`, and no module is re-exported from
  another crate. node 17 does that, and only with the `tddy-tools restructure` engine: a refusal
  means stop and ask; hand edits after a move are build corrections only, each new cause filed as a
  todo.
- **No behaviour change.** The node holds the baseline on its own: 562 passed, the same 22 failures by
  name, 1 ignored (see "Baseline").
- **Public `tddy_session_lifecycle::…` paths stay reachable.** No consumer crate is edited
  (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`). The exception is a
  test that reads lifecycle source by path; none is known for the files touched here.
- **No receiver edit.** `tddy-session-agents` is not touched in 16a. No new crate is created. No
  receiver may depend on lifecycle, and each receiver stays at or under 10k (checked in node 17).
- **No new crate edges.** Every edge node 17 needs is listed for approval here, but none is added.
  A new edge needs the developer's approval.
- **Not in 16a:** T3 (16b), T4 / `service_util` / `workspace_session` / CLI imports (16c), T9 and the
  jail and CLI-spawn half of T1 (16d), the start/resume half of T1 and T1c (16e). Their host methods
  stay host methods here. `AgentHostCallbacks`, `SplitHost` and `LaunchHost` are **not defined** here.
- **`PeerRouted*`, the port adapters, the terminal adapter and bridge stay** in lifecycle as wiring.
- **The engine is used where it can do the extraction** (`extract_module` for M0.1, M0.2 and the
  `session_notification_publishing` split). Hand conversion is allowed, because this node **is** the
  restructure. It is still restricted:
  - it may re-point a field read (`self.x` → `state.x`) or a host call (`self.m(…)` →
    `host.m(…)` or a topic function);
  - it may change a signature to take the state and port, and turn a host hand-off into a handle clone;
  - it never re-types logic, never reorders statements and never drops a comment.
- **No deduplication and no functional refactor.**
- **No re-parenting by hand without consent.** The four mixed parent/child files (M0.4) are the
  developer's call (D8).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**.
Implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `15` lifecycle-split (#526, `feature/carve/lifecycle-split`) | the leaf moves (1a, 2a, 2b, T5a+T5b, host-free T7/T8/T3 parts); `tddy_session_agents::AgentRosterState<'a>` and lifecycle's `agent_roster_state()` builder; the 11 T3 functions in `tddy-session-agents` (`clone_readiness`, `agent_clone_lookup`, `spawn_agent_def`, `hosted_clone_start`, …); the eight engine fixes; the baseline 562 / 22 / 1 | converts T7, T8, T10 and T11 on top of that layout, and cuts the M0 edges around it. (T3's use of `AgentRosterState` and the 11 functions is 16b's) | re-run or undo any #526 move, move anything across a crate, or change #526's engine fixes |
| `14` destructure (#524, merged) | every file < 500, no function > 150 except the five filed, DRY #2–#13 | converts the functions #524 laid out | split, deduplicate or re-measure; the #524 todos stay theirs |

## Draft PR contract

This is a mechanical restructure, the first of the pr-stack skill's two named exceptions ("a purely
mechanical rename / move / extraction with no behaviour change"). The draft is this plan. There are
no new failing tests. The contract is:
- the baseline, re-run after every milestone (M0–M3) at the same numbers by name;
- the 16a acceptance checks (see "Acceptance graph — after this node").

Those checks are greps plus a scripted run. They become a shape test only if the developer reverses
the 2026-09-25 "no shape tests" decision (D11).

## Green wave

**Wave:** after #526.
**Greenable independently:** **no.** It converts the tree #526 leaves, and its baseline is #526's
562 / 22 / 1. It can go green only once #526 is green and this branch is rebased onto it.
**Concurrent with:** nothing.
**Blocks:** 16b (T3 calls 16a's admission-token function and the M0.1 T3 group), and through it every
later node: 16c, 16d, 16e, then node 17.

Real dependency edges: `#526 → 16a → 16b → 16c → 16d → 16e → node 17`. Within the plan, 16d's T9
depends only on M3 (16a), and could in principle run beside 16b/16c; it is kept in the line because
its jail half calls T3 (16b) and T4 (16c) through their states.

## Prerequisites

The scan followed `deferred-work/references/planning-cross-check.md`:
- all 11 records in `packages/tddy-session-lifecycle/docs/code-issues/`;
- every `docs/dev/todo/2026-09-24-lifecycle-*` entry;
- every `*restructure*` entry.

**No record carries `Claimed by:`** (`grep -rl 'Claimed by' packages/tddy-session-lifecycle/docs/code-issues/`
is empty), so there is no claimed-issue fork to ask about.

### Code issues (`packages/tddy-session-lifecycle/docs/code-issues/`)

| Item | Verdict | What this change does about it |
|---|---|---|
| `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md` (244 lines, T) | ⚠ **During** | Already a free function, so it is not converted. Only imports are re-pointed (A4). It must not grow. node 17 moves it into `tddy-agent-launch` and re-measures |
| `complexity-daemon-rpc-handler-handle-rpc.md` (147 lines, nesting 8) | — Unrelated | Wiring that stays (a port impl). Not touched here |
| `complexity-split-claude-cli-start-start-split-claude-cli-session.md` (**146** lines) | ⚠ **During**, near the line | Converted in M5 (T4). **At 146 it has 4 lines of headroom under the 150 budget.** Re-pointing `self.x` → `state.x` can make `cargo fmt` re-wrap lines. The milestone re-measures, and must stop and ask if it crosses 150 |
| `complexity-svc-paired-codebase-teardown-delete-paired-codebase-session.md` (95) | ⚠ **During** | Converted in M5. Its `delete_session_at_session_coordinate` call becomes `host.delete_session(…)` (SplitHost). It must not grow |
| `complexity-svc-resolve-listed-worktree-ensure-project-available-for-start.md` (99, nesting 8) | ⚠ **During**; ℹ **Answered** | **Reclassified T3 → T1.** Its only caller is `provision_project_for_start` (T1). The pilot's "`tddy-spawn` edge on session-agents" blocker disappears: it goes to `tddy-agent-launch`, which needs `tddy-spawn` anyway (`spawn_tddy_coder`) |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` (119, **9 parameters**) | ⚠ **During**, at risk | Converted in M7 (T1). **Recipe A (free functions) adds a `state` and a `host` parameter: 9 → 11, which worsens the record.** Recipe B (a method on the topic handle) keeps 9. See D1 |
| `complexity-svc-resume-session-resume-session-at-session-coordinate.md` (137) | ⚠ **During** | Converted in M8 (T1c). It must not grow |
| `complexity-svc-spawn-split-agent-spawn-split-agent.md` (110, **9 parameters**) | ⚠ **During**, at risk | Same parameter risk as `resume_claude_cli_session`, under Recipe A. See D1 |
| `complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` (342, **never executed on macOS**) | ⚠ **During**, unguarded | Converted in M7. Its suites are in the baseline's known-red 22 (the sandbox RPC bridge is never installed), **so the local baseline cannot see a regression in it.** Its preservation rests on compiling, on the token-only edit rule and on Linux CI's sandboxed suites (`scripts/ci-status.sh`). See D10 |
| `complexity-svc-start-session-core-start-session-core.md` (358, fully covered) | ⚠ **During** | Converted in M7. It must not grow. Fully covered, so the baseline guards it |
| `crap-svc-start-sandboxed-cursor-cli-session.md` (CRAP 1,722, **"Restructure: no — tests first"**) | ⚠ **During**, conflicts with the record | The record forbids restructuring it before a test enters it. Converting it is a restructure of unexecuted code. Converting it anyway (Recipe B's header-only edit), adding characterisation tests first, or leaving it host-bound (which blocks all of T1's move) is **the developer's call: D10** |

### TODOs (`docs/dev/todo/`)

| Item | Verdict | What this change does about it |
|---|---|---|
| [lifecycle files over the 400-line target](../todo/2026-09-24-lifecycle-files-over-the-400-line-target.md) | ⚠ **During** | Conversions add a few lines per file. No file may reach 500. `svc_start_sandboxed_claude_cli_session.rs` (493) has **7 lines of headroom**, so it is re-measured after M7 |
| [lifecycle functions still over 150 lines](../todo/2026-09-24-lifecycle-functions-still-over-150-lines.md) | ⚠ **During** | None of the five may grow. `relaunch_sandboxed_runner` (149) and `start_split_claude_cli_session` (146) are just under the line and are re-measured |
| [lifecycle modules to re-parent by hand](../todo/2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) | ⛔ **Blocking for node 17** (resolved here with consent) | Four files are parents of a module from **another topic**. A module move takes its children, so each must be re-parented before node 17. The engine cannot (`git mv`). Scope item M0.4 and **D8** |
| [`session_entry_from_listing` not started](../todo/2026-09-24-lifecycle-session-entry-from-listing-not-started.md) | ℹ **Answered** | M8 converts `list_sessions_at_session_coordinate`. The mapping reads nothing from `self`, so it is lifted as the free function the todo names, in the same milestone. ✅ RESOLVED HERE if M8 lands it |
| [shared sandboxed jail launch needs coverage first](../todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) | ⚠ **During** | DRY #1 is not done here. The three copies (Claude, Cursor, relaunch) are converted **identically**, so the later merge still compares like with like |
| [topic files to fold into siblings](../todo/2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings.md) | — Unrelated | All four files land in the same topic as the sibling they belong in (`stack_child_spawn`, `conversation_spawn` and `stack_seed_validation` in T9, `roster_replacement` in T3). They move together, so folding them is not needed for correctness |
| [restructure has no operation to read a method's fields through a state parameter](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md) | ℹ **Answered** | This node does that edit by hand, as the restructure. The todo stays open as an engine capability |
| [restructure has no signature operations](../todo/2026-09-24-restructure-has-no-signature-operations.md) | ⚠ **During** | Every conversion changes a signature by hand. Each change is listed in the milestone's commit |
| [move-to-crate reads an import reaching the destination as an edge](../todo/2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md) | ⛔ **Blocking for node 17**; mitigated here | It blocked three T3 methods in the pilot. The same shapes (`super::` paths, module imports through lifecycle re-exports) are all over T4 and T1. **Acceptance check A4** makes every topic module import foundations and lower topics by their defining crate or its own path, so node 17's moves do not present that shape. The engine fix itself is not in scope |
| [extract drops comments and writes clippy-failing signatures](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) | ⚠ **During** | Where the engine extracts (P, Q), comments are restored and signatures reshaped, as #524 did. Hand conversion keeps every comment |
| [apply leaves the lint gate red](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) | ⚠ **During** | clippy `-D warnings` and `fmt --check` on lifecycle after every milestone |
| [extract-method: a return before a unit-`if` tail](../todo/2026-09-25-restructure-extract-method-accepts-a-return-before-a-unit-if-tail.md), [function-local `use` left behind](../todo/2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind.md), [extract-variable hoists a borrowed field by value](../todo/2026-09-25-restructure-extract-variable-hoists-a-borrowed-field-by-value.md), [extract-variable waits for ever on `&`](../todo/2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md) | ⚠ **During** | Known engine traps when the engine is used here. Each is avoided or corrected as #526 did. No new todo unless a new cause appears |
| [check misses a body path to a module staying behind](../todo/2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md), [check misses a module name the destination already has](../todo/2026-09-25-restructure-check-misses-a-module-name-the-destination-already-has.md) | ⚠ **During** (for node 17) | Checks A2/A4 are the manual version of the body-path check. The collision survey is in "New crate edges". Only `session_notifications` collides, and M2 renames the half that moves |
| [glob facade re-exports a name the origin shadows](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md), [destination's own extern name](../todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md), [follows a facade back](../todo/2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md), [crate named only in a body path](../todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md), [nested module's parent glob dangling](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md), [test module's use lines](../todo/2026-09-25-restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module.md), [test binary cannot see through a glob facade](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md) | — for this node; ⚠ for node 17 | Cross-crate move defects. None applies to an in-place conversion. node 17 will meet them as known build corrections |
| [restructure defects from the first cross-crate move](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) (item 3: `pub(crate)` → `pub`) | ⚠ for node 17 | This node keeps `pub(crate)` visibilities as they are. node 17 widens what crosses a crate |
| [restructure verify cannot exit zero for an extract module](../todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md) | ⚠ **During** | `verify --against` is accounted by hand per milestone |
| [restructure snapshot cannot rebase a stale plan](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md) | — Unrelated | Plans are written after the milestone before them lands |
| [2026-09-09 macro expansion as an operation](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md), [2026-09-09 defects from the connection-service split](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md) | — Unrelated | Research and an earlier run's findings; nothing here depends on them |
| [2026-09-09 untested complexity hotspots](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **During** (carried from #526) | Re-measured per owning crate after node 17 |

**Packages without `docs/code-issues/`:** `tddy-session-agents` has none. So will the new crates of
node 17. Run `/analyze-code-issues` on each once code lands there.

## Scope

This node (16a) is M0–M3. M4–M8 are the successor nodes' scope (see "Milestones and the PR cut" and
"Successor PRs").

- [ ] **M0: baseline and cuts.** Record the baseline, then cut the six wrong-way cross-topic edges that
  need no port. The other four in "Cross-topic call matrix" (`start_session_core` and
  `delete_session_…` behind `SplitHost`, `attached_initial_prompt` and `PairedAgentSession` by
  placement) are cut in 16c:
  - [ ] M0.1 move the free predicates `peer_has_no_such_session` and `split_pairing`, and the free
    functions `resolve_worktree_root_for_session` and `resolve_exec_tool_worktree`, into a T3 module,
    `peer_session_answer` (`extract_module`, in lifecycle). The facades that name them keep resolving;
  - [ ] M0.2 group the daemon-URL trio (`advertise_daemon_url`, `local_daemon_hook_url`,
    `claude_hook_daemon_url`) into a `daemon_urls` module. Group `write_claude_hooks_settings` and
    `resolve_start_session_claude_binary` with T4. `cursor_cli_spawn/chat.rs`'s import re-points to
    `daemon_urls`;
  - [ ] M0.3 turn `split_forward_deadline`, `session_dir_for` and `mint_first_admission_token` into
    free functions of the fields they read. The host methods stay as delegators, in a wiring file;
  - [ ] M0.4 re-parent the four mixed parent/child files (**consent needed, D8**);
  - [ ] M0.5 move `svc_split_context_from_codebase_host.rs`'s host-constructing inline test out to a
    `connection_service/split_context_from_codebase_host_tests.rs` sibling that stays in lifecycle (D9);
  - [ ] M0.6 split `seeded_clone_guard.rs`: move `SessionStdioEndpoint` to T1 and `ExecToolRoute` beside
    `LocalExecTools`.
- [ ] **M1: T11 demo VM**: `DemoVmState` {`demo_vm_state`, `tddy_data_dir`, `user_resolver`,
  `rpc_activity`, `config`}. `DemoVmServiceImpl` holds it instead of `Arc<DaemonSessionHost>`;
  `demo_vm_entry` (wiring) builds it
- [ ] **M2: T10 presenter**: `PresenterObserverDeps` {`tddy_data_dir`, `presenter_event_sink`,
  `session_notification_bus`}. Lifecycle's `session_notifications` publishing half
  (`SessionNotificationPublishing`, `resolve_session_label`) moves into a non-colliding module,
  `session_notification_publishing`, behind the existing facade
- [ ] **M3: T7 and T8**: the admission-token function over `config` and `session_admissions` (no state
  struct, D6); the `resolve_os_user` free function grouped into its own module; `AttachmentState`
  {`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`} for the two attachment files
- [ ] **Leaves** (`placement`, `remote_git_pack_execution`, `agent_list_mapping`, `daemon_urls`): no
  conversion. Imports re-pointed to their defining crates only (A4)
- [ ] **Baseline** after every milestone: 562 / 22 / 1, the same 22 by name. clippy and fmt clean on
  lifecycle. `cargo check --all-targets` clean on lifecycle, `tddy-session-agents`, `tddy-daemon-rpc`,
  `tddy-daemon` and `tddy-telegram-control`
- [ ] **Acceptance checks** A1–A8 for 16a's topics pass (see "Acceptance graph — after this node")
- [ ] `restructure verify --against <milestone base>`: every statement accounted for

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Where every part of tddy-session-lifecycle goes

### How lines were counted

This uses the discovery's definition of a **production line**: any line of a `src/` file outside an
inline `#[cfg(test)] mod x { … }` block. A file whose module is declared behind `#[cfg(test)]` (the 18
`connection_service/*_tests.rs` files) counts as test code in full. Brace depth is tracked from the
`#[cfg(test)]` line to the closing brace of the module it gates.

It reproduces **20,041** production lines at `9d464a8e`, **exactly #526's closing figure**, across 128
`.rs` files: 110 carry production code, and 25,592 lines in total. Method-level splits of mixed files
count each method from its `fn` line to its closing brace. Test modules are excluded, and the rest
of the file goes to the file's main topic.

### Headline: where each topic ends

| Topic | Production lines | Final crate (after node 17) |
|---|---:|---|
| **T1** agent launch: CLI start/resume, sandboxed jail starts, relaunch, tool spawn, managed launch, hooks | **5,500** | `tddy-agent-launch` (new) |
| **T9** stack, child and conversation spawns; PR-stack links | 902 | `tddy-agent-launch` |
| **T1c** session coordinate handlers: list, start, stream-start, connect, resume, signal, delete, snapshot | 766 | `tddy-agent-launch` |
| **CLI** PTY runtime: `cli_session_manager` (10 files), `session_toolcall` | 1,709 | **`tddy-cli-sessions`** (new, **proposed**, D4) |
| **T4** split and sandboxed-codebase sessions | 2,485 | `tddy-session-split` (new) |
| `service_util` | 293 | `tddy-session-split` (D5) |
| `workspace_session` (all but `resolve_worktree_root_for_session`) | 266 | `tddy-session-split` (D5) |
| **T3** agent clones, roster, agent-def resolution | 2,054 | `tddy-session-agents` |
| `local_exec_tools` (`LocalExecTools`) | 231 | **stays** (approved callbacks), or `tddy-session-agents` (D7) |
| **T8** attachments (host half) | 370 | `tddy-session-files` |
| **T10** presenter observation | 265 | `tddy-session-activity` |
| lifecycle's `session_notifications` | 96 | publishing half (~70) → `tddy-session-activity`; facade (~26) stays |
| `remote_git_pack_execution` (host-free leaf; in no plan) | 86 | `tddy-session-activity` |
| **T7** admission token, `resolve_os_user` free fn | 58 | `tddy-daemon-livekit` |
| `placement` (host-free leaf) | 155 | `tddy-daemon-livekit` |
| kernel leaves: `agent_list_mapping`, the daemon-URL trio | 48 | `tddy-daemon-kernel` |
| **T11** demo VM | 313 | `tddy-demo-runner` (D12) |
| `test_util` | 366 | gated behind a feature, or moved to a testkit (node 17) |
| **W** wiring: host struct, builders, port impls, `PeerRouted*`, adapters, `SessionService`/`SessionHandler`, `daemon_rpc_handler`, terminal adapter and bridge, facades | 4,078 | **stays** |
| **Total** | **20,041** | |

### Per-file inventory

The columns are:
- **#16**: what the conversion (16a–16e) does. It names the state the body reads and the callback methods it calls.
  "No change" means the file is already free of the host.
- **#17**: the engine operation in the next node.
- **Blockers**: what stands in the way.

The column abbreviations are:
- **LS** `LaunchState`, **AR** `AgentRosterState` (with its owned handle), **SS** `SplitState`,
  **AS** `AttachmentState`;
- **LH** `LaunchHost`, **AHC** `AgentHostCallbacks`, **SH** `SplitHost`.

Paths are under `packages/tddy-session-lifecycle/src/`, and `cs/` is `connection_service/`.

#### T1: agent launch → `tddy-agent-launch` (5,500 lines, plus 902 in T9 and 766 in T1c)

Converted in two halves: **M7a** (16d) the jails and CLI spawns, **M7b** (16e) the start/resume
path. The file-by-file split is under "Milestones and the PR cut".

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cs/svc_start_session_core.rs` | 455 | `start_session_core` (358) → LS. It calls T3 (`agent_def_for_spawn`, `seeded_roster_records`, `unwind_seeded_roster`) through AR, T4 (`start_sandboxed_codebase_session`, `start_split_claude_cli_session`, `provision_workspace_tool_sandbox`) through SS, and T8 (`prepare_session_attachments`) through AS. `seed_clone_claimant` comes from the agents handle (D3) | cluster move with T1 | code issue (358, E4 guards) |
| `cs/svc_start_session_core/cli_branch_starts.rs` | 220 | 6 methods → LS. `attached_initial_prompt` is also called by T4, so it moves to split (D3) | with T1 | T4 → T1 edge, cut by moving `attached_initial_prompt` |
| `cs/svc_start_session_core/start_request_checks.rs` | 98 | 4 methods → LS (`config`, `peer_routing`, `tddy_data_dir`) | with T1 | — |
| `cs/svc_start_session_core/tool_session_spawn.rs` | 228 | `spawn_tool_session`, `spawn_tddy_coder` → LS (`spawn_client`). It calls T10 through `PresenterObserverDeps` and T8 through AS | with T1 | — |
| `cs/svc_start_session_core/tool_spawn_plan.rs` | 67 | no change | with T1 | — |
| `cs/svc_start_session_core/workspace_branch_start.rs` | 122 | 3 methods → LS. It calls T3 (`seed_session_agent_roster`, `start_hosted_agent_clone`, …) and `workspace_session` (split) | with T1 | — |
| `cs/svc_start_claude_cli_session.rs` | 236 | 3 methods → LS. **3 × `self.clone()`**: `StackChildSpawnHandler`, `GrillMeConversationSpawnHandler` and the host-session socket each get the owned launch handle | with T1 | `self.clone()` to tasks (D1) |
| `cs/claude_cli_spawn.rs` | 285 | no change: already free. Imports re-pointed (A4) | with T1 | code issue (255, T) |
| `cs/claude_cli_spawn/claude_cli_spawn_steps.rs` | 300 | no change (free) | with T1 | — |
| `cursor_cli_spawn.rs` | 445 | no change (free) | with T1 | code issue (244) |
| `cursor_cli_spawn/chat.rs` | 94 | imports only: `local_daemon_hook_url` → `daemon_urls` (M0.2) | with T1 | — |
| `cursor_cli_spawn/resume.rs` | 96 | no change (free) | with T1 | — |
| `cs/svc_start_sandboxed_claude_cli_session.rs` | 493 | `start_sandboxed_claude_cli_session` (342), `jail_runner_env` → LS. It calls T3 `claim_co_located_seed_clones` and T9 stack links | with T1 | **untested on macOS** (D10); 7 lines of headroom under 500 |
| `cs/svc_start_sandboxed_claude_cli_session/jail_launch_steps.rs` | 221 | 4 methods → LS. `launch_jail` calls `sandbox_rpc_handler` (LH). `warm_up_jail_agents` calls T3 through AR | with T1 | untested on macOS |
| `…/jail_session_files.rs` | 104 | no change (free) | with T1 | — |
| `…/jail_worktree.rs` | 136 | 3 methods → LS. They call T9 (`resolve_chain_base_ref_status`, `record_spawn_on_stack_node`) | with T1 | — |
| `cs/svc_start_sandboxed_cursor_cli_session.rs` | 445 | `start_sandboxed_cursor_cli_session` (414) → LS, LH `sandbox_rpc_handler`, AR | with T1 | **crap record: "tests first"** (D10) |
| `cs/svc_relaunch_sandboxed_runner.rs` | 213 | `relaunch_sandboxed_runner` (149) → LS | with T1 | 1 line under 150 |
| `…/relaunch_jail_dirs.rs` | 73 | no change (free) | with T1 | — |
| `…/relaunch_jail_steps.rs` | 229 | 5 methods → LS. LH `sandbox_rpc_handler` | with T1 | — |
| `cs/svc_resume_claude_cli_session.rs` | 272 | **mixed.** T1 (172): `resume_claude_cli_session` (119) → LS, which calls T4 `resume_split_wiring` through SS. T4 (100): `resume_split_wiring`, `split_withdrawals_from_codebase_host` → SS, `SH::session_agents` | T1 part with T1. T4 part moves in M5 via `extract_module` into a T4 module | code issue (9 params, D1) |
| `cs/svc_turn_end_reporter/jail_env_builders.rs` | 71 | 3 methods → LS (`config`) | with T1 | **child of a T3 file** (D8) |
| `cs/managed_launch.rs` | 95 | no change (free) | with T1 | — |
| `cs/worktree_source.rs` | 40 | no change (free) | with T1 | — |
| `cs/hooks_and_urls.rs` | 227 | **mixed.** The T1 part (~186) stays. The URL trio (22) goes to `daemon_urls` → kernel. `write_claude_hooks_settings` (16) and `resolve_start_session_claude_binary` (3) go to T4 | with T1 | T3 → T1 (`advertise_daemon_url`) and T4 → T1 (hooks, URLs): cut in M0.2 |
| `cs/svc_pr_status_for_caller.rs` (T1 part) | 104 | `managed_resume_goal`, `prepare_managed_workflow`, `owned_branch_conflict` → LS | with T1 | the file is mixed with T9, and both go to the same crate |
| `cs/svc_resolve_listed_worktree.rs` (T1 part) | 185 | `ensure_project_available_for_start` (99), `spawn_project_clone` → LS (`spawn_client`, `peer_routing`) | `extract_module` in M7 into a T1 module, then with T1 | the file is mixed with T3 |
| `cs/svc_split_context_from_codebase_host.rs` (T1 part) | 62 | `resume_sandboxed_claude_cli_session` → LS (`sandbox_manager`) | `extract_module` in M7 | the file is mixed with T4 |
| `cs/svc_ensure_session_room_for_agents.rs` (T1 part) | 22 | `index_workspace_worktree` → LS | `extract_module` | the file is mixed with T3 |

#### T9: stack spawns → `tddy-agent-launch` (902 lines)

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cs/svc_pr_status_for_caller.rs` (T9 part) | 226 | `resolve_chain_base_ref_status`, `link_stack_node_to_spawned_branch` and `record_spawn_on_stack_node` → LS, plus **LH `pr_stack`** (they read `rpc_families()?.pr_stack_handler()`) | with T1 | — |
| `cs/stack_parent.rs` | 255 | free functions stay. **`impl StackParentHost for DaemonSessionHost`** becomes an impl on the owned launch handle, since its two delegations are T9's own | with T1 | trait impl on the host (moves off it) |
| `cs/child_spawn_handler.rs` | 131 | `StackChildSpawnHandler::spawn_child` reads `service` (the host): `prepare_session_attachments` (T8), `claude_cli_manager`, `config`, `tddy_data_dir` → the owned handle | with T1 | `self.clone()` hand-off (D1) |
| `cs/stack_child_spawn.rs` | 35 | the struct's `service: DaemonSessionHost` → the owned launch handle | with T1 | — |
| `cs/conversation_spawn.rs` | 64 | no change (free) | with T1 | — |
| `cs/conversation_spawn_handler.rs` | 87 | `stack_parent_host: Arc<host>` → the handle | with T1 | — |
| `cs/stack_seed_validation.rs` | 104 | no change (free; calls `project_repo_root`, which is `service_util`) | with T1 | — |

#### T1c: session coordinate handlers → `tddy-agent-launch` (766 lines)

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cs/session_coordinate_handlers.rs` | 414 | 5 methods → LS:<br>• `list_sessions_…` (129: `agent_activity_hub`, `session_agent_inference`), plus the `session_entry_from_listing` lift;<br>• `connect_…`, which calls T3 `ensure_session_room`;<br>• `get_worktree_snapshot_…`, which calls `resolve_exec_tool_worktree` and `rpc_served_by_peer` (both become state reads);<br>• `stream_start_…`: `self.clone()` into a task, which becomes a handle clone;<br>• `start_…` | with T1 | `self.clone()` (D1) |
| `…/svc_resume_session.rs` | 166 | `resume_session_at_session_coordinate` (137) → LS. It calls T1, T3 (AR), T4 (SS) and T10 | with T1 | code issue (137) |
| `…/svc_signal_delete_session.rs` | 186 | `signal_…` (87) → LS. `delete_…` (72) → LS; it reads `hosted_agent_clones`, `sandbox_manager`, `session_admissions`, `session_agent_inference`, `session_rooms` and `workspace_sandboxes`, and calls T3 `tear_down_every_agent_clone` and T4 `delete_paired_codebase_session` | with T1 | — |

#### CLI: PTY runtime → `tddy-cli-sessions` (new, proposed; 1,709 lines)

**Why a crate of its own:** T4 (split) names `CliSessionManager`, its `start_with_options`, and
`PtyHandle` (`svc_spawn_split_agent.rs:221,424`). Split must sit below `tddy-agent-launch`, so the
PTY runtime must sit below split. It cannot go into `tddy-terminal-rpc`: it names `session_toolcall`
(`tddy-daemon-sandbox`, `tddy-stdio`) and `session_deletion::signal_pid` (`tddy-session-activity`),
which would drag the sandbox stack into `tddy-coder`.

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cli_session_manager.rs` | 173 | no change. Imports re-pointed (A4) | `move_cluster_to_crate` → `tddy-cli-sessions` (anchor), with its 9 children and `session_toolcall` | new crate needs approval (D4) |
| `cli_session_manager/{argv, control_lease, launch, livekit_bridge, livekit_terminals, pty_handle, pty_spawn, relaunch, terminals}.rs` | 83, 89, 179, 230, 99, 117, 216, 70, 206 | no change. `pty_handle` and `pty_spawn` name `crate::pty_runtime` → `tddy_terminal_rpc::pty_runtime`; `terminals` names `crate::session_deletion` → `tddy_session_activity::…` (A4) | in the cluster | — |
| `session_toolcall.rs` | 247 | no change | in the cluster | — |

#### T4: split, `service_util`, `workspace_session` → `tddy-session-split` (2,485 + 293 + 266)

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cs/svc_spawn_split_agent.rs` | 445 | 8 methods → SS:<br>• `spawn_split_agent` (110);<br>• `spawn_split_agent_process` reads `claude_cli_manager`;<br>• `join_split_livekit_room`: **2 × `self.clone()`**, for `RemoteCheckout::new(Arc::new(self.clone()))` (becomes `SH` / AHC `worktree_snapshot`) and the room-roster closure (becomes `SH::session_room_roster`);<br>• `agent_session_token_for` and two sites read `session_tokens()` (becomes an SS field and a refusal fn);<br>• `split_forward_deadline` becomes a free fn (M0.3);<br>• `write_split_agent_metadata` is free | cluster → `tddy-session-split` | code issue (110, 9 params) |
| `cs/svc_spawn_split_agent/svc_paired_codebase_teardown.rs` | 178 | `tear_down_codebase_session`, `delete_paired_codebase_session` (95) → SS, **`SH::delete_session`** (it re-enters `delete_session_at_session_coordinate`, T1c) | in the cluster | code issue (95) |
| `cs/svc_materialize_staged_attachment/split_claude_cli_start.rs` | 178 | `start_split_claude_cli_session` (146) → SS. It calls T3 `resolve_specialized_agent_defs` through AR and `split_forward_deadline` (free) | in the cluster | code issue (146 near 150); **child of a T8 file** (D8) |
| `cs/svc_split_context_from_codebase_host.rs` (T4 part) | 409 | `split_context_from_codebase_host` (132) → SS. `context_manifest_of`, `context_file_batch_of` and `session_files_of_this_daemon` hand `Arc::new(self.clone()).session_files_service()` (`PeerRoutedSessionFiles`, wiring) → **`SH::session_files`** (new, D3) | in the cluster | inline test constructs `DaemonSessionHost` (M0.5, D9) |
| `cs/svc_start_sandboxed_codebase_session.rs` | 267 | 4 methods → SS. `start_sandboxed_codebase_session` calls `start_session_core` → **`SH::start_workspace_session`** (the T1↔T4 cut). `reprovision_colocated_checkout_jail` calls `provision_workspace_tool_sandbox` (T4) | in the cluster | — |
| `cs/split_start.rs` | 131 | T4 (125) no change, except it names `workspace_session::PairedAgentSession` (so `workspace_session` goes to split, D5). `peer_has_no_such_session` (6) → T3 (M0.1) | in the cluster | T3 → T4 edge (cut in M0.1) |
| `cs/svc_resolve_tddy_tools_path.rs` | 51 | `agent_tool_socket_for_embedded_host`, `resolve_tddy_tools_path` → free fns over `tddy_data_dir`/`config`. **The plan's `SplitHost::agent_tool_socket` is not needed** | in the cluster | **parent of wiring's `svc_host_builders`** (D8); `resolve_tddy_tools_path` is a public facade |
| `split_session.rs` | 409 | T4 (400) no change (free). `split_pairing` (9) → T3 (M0.1): `agent_roster.rs:230` calls it | in the cluster | T3 → T4 edge (cut in M0.1) |
| `split_session/agent_argv.rs`, `split_session/agent_credentials.rs` | 176, 113 | no change (free). `agent_argv` calls T3's `roster_replacement_pairs` | in the cluster | — |
| `cs/svc_resume_claude_cli_session.rs` (T4 part) | 100 | see T1 | `extract_module` in M5 | — |
| `cs/svc_ensure_session_room_for_agents.rs` (T4 part) | 27 | `provision_workspace_tool_sandbox` → SS (`workspace_sandboxes`, `workspace_sandbox_provisioner`) | `extract_module` | — |
| `cs/hooks_and_urls.rs` (`write_claude_hooks_settings`, `resolve_start_session_claude_binary`) | 16 + 3 | grouped with T4 (M0.2) | in the cluster | — |
| `cs/service_util.rs` | 293 | no change (free) | in the cluster | its two `pub use` (`spawn_blocking_with_timeout`, `await_supervised_with_timeout`) are read by `tddy-daemon-rpc`, and the facade must be kept |
| `workspace_session.rs` | 279 | 266 no change (free). `resolve_worktree_root_for_session` (13) → T3 (M0.1): T3, T7's `resolve_exec_tool_worktree`, `session_agent_clone` and `tddy-daemon/src/runtime.rs` call it | in the cluster | T3 → WS edge (cut in M0.1); `tddy-daemon` reaches it through the facade |

#### T3: agents → `tddy-session-agents` (2,054 lines)

| File | Lines | #16 | #17 | Blockers |
|---|---:|---|---|---|
| `cs/svc_provision_agent_clone.rs` | 382 | 12 methods → AR:<br>• `provision_agent_clone` → `peer_routing.common_room_slot`, `mint_first_admission_token` (free fn over `config` and `session_admissions`), `split_forward_deadline` (free) and `daemon_urls::advertise_daemon_url`;<br>• `delete_clone_on_peer` and `tear_down_agent_clone` use `peer_session_answer` (M0.1);<br>• `hosted_clone_for` and `run_hosted_clone_tool` → **AHC `local_exec_tools`** ✅ | cluster → session-agents | the pilot's import refusal (A4 mitigates) |
| `cs/svc_ensure_session_room_for_agents.rs` (T3 part) | 366 | 6 methods → AR:<br>• `claim_agent_clone`: `self.clone()` into `tokio::spawn`;<br>• `claim_co_located_seed_clones`: `self.clone()` into `SeededCloneGuard`;<br>both become an owned-handle clone | in the cluster | `self.clone()` (D1) |
| `cs/svc_start_hosted_agent_clone.rs` | 334 | 9 methods → AR:<br>• `owned_`/`local_agent_codebase_access`: `'static` closures holding `self.clone()` → owned handle; they call **AHC `run_exec_tool_locally`** ✅ and `local_exec_tools` ✅, and `resolve_exec_tool_worktree` becomes a free fn over AR;<br>• the `start_hosted_agent_clone` head's `workspace_session` call → T3's own `resolve_worktree_root_for_session` (M0.1) | in the cluster | `self.clone()` (D1) |
| `cs/svc_resolve_listed_worktree.rs` (T3 part) | 248 | `report_shadowed_agent_def`, `resolvable_agent_defs` (method and free fn), `agent_def_for_spawn`, `resolve_specialized_agent_defs`, `seeded_roster_records`, `roster_session_dir` → AR (+ `model_registry`) | in the cluster | the engine refused `report_shadowed_agent_def` (early return); hand conversion is fine here |
| `cs/svc_resolve_listed_worktree/session_dir_lookup.rs` | 23 | `session_dir_for` → a free fn over `tddy_data_dir` (M0.3) | in the cluster | — |
| `cs/svc_resolve_listed_worktree/session_room_opening.rs` | 41 | `ensure_session_room` → AR (`config`, `session_rooms`), **AHC `session_room_roster`** (new, D2) for `Arc::new(self.clone()).session_room_roster()` | in the cluster | unapproved callback |
| `cs/svc_turn_end_reporter.rs` | 158 | 4 methods → AR. `remote_roster_record_for` → `peer_routing.common_room_slot` and `.eligible_instance_ids` | in the cluster | **parent of T1's `jail_env_builders`** (D8) |
| `cs/agent_roster.rs` (T3 part) | 198 | free functions: no change. `session_enforces_a_withdrawal` → T3's `split_pairing` (M0.1) | in the cluster | wiring part (44) stays, see W |
| `cs/seeded_clone_guard.rs` | 136 | `SeededCloneRelease.service: DaemonSessionHost` → the owned handle. `SessionStdioEndpoint` (~8, T1) and `ExecToolRoute` (~13) move out (M0.6) | in the cluster | mixed file |
| `cs/seed_codebase.rs` | 98 | no change (`SeedCodebase`, `SeededAgentClones` trait) | in the cluster | — |
| `cs/roster_replacement.rs` | 24 | no change | in the cluster | — |
| `cs/split_start.rs` (`peer_has_no_such_session`), `split_session.rs` (`split_pairing`), `workspace_session.rs` (`resolve_worktree_root_for_session`), `cs/svc_resolve_os_user.rs` (`resolve_exec_tool_worktree` free fn, 18) | 6, 9, 13, 18 | grouped into T3 modules (M0.1) | in the cluster | — |

#### T8, T7, T10, T11 and host-free leaves

| File | Lines | Topic → final crate | #16 | #17 | Blockers |
|---|---:|---|---|---|---|
| `cs/svc_materialize_staged_attachment.rs` | 245 | T8 → `tddy-session-files` | 4 methods → **AS** {`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`}. `classify_daemon_route` and `common_room_slot` become `peer_routing.…` reads. **No callback** | `move_module_to_crate` | **parent of T4's `split_claude_cli_start`** (D8); new edge session-files → `tddy-daemon-livekit` (`PeerRoute`, `local_instance_id_for_config`) |
| `cs/svc_resolve_os_user/session_attachment_materialization.rs` | 125 | T8 → session-files | `prepare_session_attachments`, `materialize_session_attachments` → AS | with T8 | **child of the wiring's `svc_resolve_os_user.rs`** (D8) |
| `cs/svc_resolve_tddy_tools_path/svc_host_builders/first_admission_token.rs` | 46 | T7 → `tddy-daemon-livekit` | `mint_first_admission_token` → free fn over `config` and `session_admissions`. **No `AdmissionState` struct** (D6): only these two fields are read, and the planned `OsUserResolver` name collides with activity's type alias | `move_module_to_crate` | nested under wiring's builders; moves individually |
| `cs/svc_resolve_os_user.rs` (`resolve_os_user` free fn) | 12 | T7 → livekit | grouped into a module (`extract_module`) | with T7 | public facade `connection_service::resolve_os_user` |
| `cs/svc_resolve_tddy_tools_path/svc_host_builders/presenter_observer_spawn.rs` | 47 | T10 → `tddy-session-activity` | `maybe_spawn_presenter_observer` → **`PresenterObserverDeps`** {`tddy_data_dir`, `presenter_event_sink`, `session_notification_bus`}, exactly as planned | cluster with T10 | nested under wiring's builders |
| `presenter_observer_task.rs` | 120 | T10 → activity | imports `SessionNotificationPublishing` from the new `session_notification_publishing` (M2) | cluster → activity | needs `tonic` on activity (approved) |
| `presenter_intent_client.rs` | 98 | T10 → activity | no change | in the cluster | — |
| `session_notifications.rs` | 96 | T10 → activity (publishing half) | `SessionNotificationPublishing`, `resolve_session_label` → new `session_notification_publishing` (`extract_module`). The facade keeps `crate::session_notifications::X` | move the new module | **name collision** with activity's `session_notifications`, avoided by the new name |
| `cs/demo_vm_coordinate_handlers.rs` | 241 | T11 → `tddy-demo-runner` | 3 methods → **`DemoVmState`** {`demo_vm_state` (vms), `tddy_data_dir`, `user_resolver`, `rpc_activity`, **`config`**}. `config` is new vs the plan: `os_user_for_github` | `move_cluster_to_crate` | new edges on demo-runner (D12) |
| `cs/activity_hub.rs` | 19 | T11 → demo-runner | no change (`DemoVmHandle`) | in the cluster | — |
| `cs/svc_demo_vm_ports.rs` | 61 | T11 (53) / W (8) | `DemoVmServiceImpl` holds an owned `DemoVmState`, not `Arc<DaemonSessionHost>`. `demo_vm_entry` (wiring) builds it | in the cluster; `demo_vm_entry` stays | `tddy-daemon/src/runtime.rs` names `DemoVmServiceImpl` and `demo_vm_entry()`; the facade is kept |
| `cs/placement.rs` | 155 | leaf → `tddy-daemon-livekit` | no change (host-free; names only `livekit_peer_discovery`) | `move_module_to_crate` | public facade `connection_service::*` (daemon-rpc tests) |
| `remote_git_pack_execution.rs` | 86 | leaf → `tddy-session-activity` | no change (`session_reader`, `project_storage` and `worktree_service` are all activity's already) | `move_module_to_crate` | none: zero edges. **In no plan until now** |
| `agent_list_mapping.rs` | 26 | leaf → `tddy-daemon-kernel` | no change | `move_module_to_crate` | zero edges (`tddy-discovery`, `tddy-service` are the kernel's) |
| `cs/hooks_and_urls.rs` (URL trio) | 22 | leaf → kernel | grouped into `daemon_urls` (M0.2) | `move_module_to_crate` | `effective_spawn_branch` stays in T1, and daemon-rpc reads it through the facade |

#### W: wiring that stays (4,078 lines), and `test_util`, `local_exec_tools`

| File | Lines | #16 | #17 |
|---|---:|---|---|
| `connection_service.rs` | 490 | host struct (34 fields), `mod` declarations, `DaemonRpcHandler`, the two `const _: assert!`, `activity_delta_frames`. Gains the ports-module declarations | gains facade lines |
| `cs/handler_state.rs` | 114 | 12 accessors, `local_exec_tools()`, `agent_roster_state()`. **Gains the state builders** (SS, LS, AS, `DemoVmState`, `PresenterObserverDeps`, the owned handles) | unchanged |
| `cs/svc_resolve_tddy_tools_path/svc_host_builders.rs` + `rpc_activity.rs` | 367 + 8 | `new`, `with_*`/`set_*`, `routing_view`. `seed_clone_claimant` re-pointed (D3) | re-parented (D8) |
| `cs/svc_activity_ports.rs` | 384 | `PeerRoutedActivity` (~155) and the activity adapters. Unchanged | stays |
| `cs/svc_session_agent_ports.rs` + `…/svc_peer_routed_session_agents.rs` + `…/svc_session_agent_port_adapters.rs` | 113 + 278 + 277 | the adapters call 15 T3 host methods (`claim_agent_clone`, `open_local_agent_session`, …) → **re-pointed** to the agents handle (or delegators, D1) | stays |
| `cs/svc_session_files_ports.rs` + `…/svc_peer_routed_session_files.rs` | 200 + 358 | `session_room_roster` (every service entry; the source of `AHC`/`SH::session_room_roster`), `session_context_scope`. Unchanged | stays |
| `cs/svc_session_lifecycle_ports.rs` | 166 | the `SessionHandler`/`SessionService` impls → re-pointed to the launch handle (M8) | stays |
| `cs/svc_terminal_ports.rs`, `terminal_session_adapter.rs`, `cs/terminal_bridge_impl.rs` | 176 + 213 + 59 | unchanged (T6c) | stay; name `tddy_cli_sessions::CliSessionManager` after node 17 |
| `cs/svc_resolve_os_user.rs` (wiring part) | 157 | 8 one-line delegations to `PeerRouting` and the free fns, plus `context_globs_for_session`. `PeerRouted*` still use them | stays; re-parent its T8 child (D8) |
| `cs/svc_resolve_os_user/local_exec_tool_dispatch.rs` | 25 | `run_exec_tool_locally`: the AHC impl source | stays |
| `cs/agent_roster.rs` (wiring part) | 44 | `impl RemoteSnapshotSource for DaemonSessionHost` (the AHC `worktree_snapshot` source), `DaemonSeedCloneClaimant` → holds the agents handle (D3) | `extract_module` in M4, so the T3 part can move |
| `cs/daemon_rpc_handler.rs`, `cs/family_proto_bridge.rs`, `cs/svc_shut_down_children.rs` | 165 + 14 + 32 | unchanged | stay |
| `lib.rs`, `handler.rs`, `service.rs`, `rpc_families.rs`, `pr_stack_rpc.rs`, `claude_cli_session.rs`, `session_agent_clone.rs` | 158 + 61 + 101 + 66 + 7 + 6 + 31 | unchanged. `session_agent_clone::clone_worktree_path` re-points to T3's `resolve_worktree_root_for_session` | gain named facades |
| `cs/local_exec_tools.rs` | 231 | unchanged. It is the approved AHC `local_exec_tools` source | stays (D7) |
| `test_util.rs` | 366 | unchanged | gated or moved to a testkit |

### State structs and callback traits

Every field and callback below comes from grepping the bodies for `self.<field>`,
`self.<method>(`, and the same through a cloned handle (`service.…`, `self.service.…`, across line
breaks). ✅ marks approved, ❌ not approved, **new** means not in the plan.

#### `AgentRosterState` (exists, `tddy-session-agents`) and `AgentHostCallbacks` (T3)

| | Content |
|---|---|
| Fields today (10) | `config`, `tddy_data_dir`, `user_resolver`, `peer_routing`, `room_roster`, `session_rooms`, `session_agent_rosters`, `session_agent_clones`, `hosted_agent_clones`, `roster_keepalive_interval` |
| Fields the bodies also read | **`session_admissions`** (`tear_down_agent_clone`, and the admission-token fn) and **`model_registry`** (`resolvable_agent_defs`, `agent_def_for_spawn`; the pilot hoisted it as a parameter) |
| Owned variant | needed. `claim_agent_clone` (`tokio::spawn`), `claim_co_located_seed_clones` (`SeededCloneGuard`), `owned_`/`local_agent_codebase_access` (`'static` closures) and `ensure_session_room` (room-roster closure) hand `self.clone()` to something `'static`. A borrowed state cannot go there. See D1 |
| AHC `local_exec_tools` ✅ | `hosted_clone_for`, `run_hosted_clone_tool` |
| AHC `run_exec_tool_locally` ✅ | `local_agent_codebase_access`'s closure |
| AHC `worktree_snapshot` ✅ | **no T3 body calls it.** The caller is T4's `join_split_livekit_room` (`RemoteCheckout` needs a `RemoteSnapshotSource`). Kept, and `SplitHost` inherits it |
| AHC `session_room_roster` ❌ **new** | `ensure_session_room` (`|| Arc::new(self.clone()).session_room_roster()`). The service-entry list is wiring |

The **seven further host methods** the pilot listed are not callbacks. Six are state reads or free
functions of state fields, and the seventh reduces to `session_room_roster`:

| Pilot's candidate | Call sites | Becomes |
|---|---|---|
| `common_room_slot` ❌ | `provision_agent_clone`, `delete_clone_on_peer`, `tear_down_agent_clone`, `forward_open_…`, `forward_cancel_…`, `remote_roster_record_for` | `state.peer_routing.common_room_slot(…)`. The host method is a one-line delegation (`svc_resolve_os_user.rs`) |
| `eligible_instance_ids` ❌ | `refuse_departed_daemon`, `remote_roster_record_for` | `state.peer_routing.eligible_instance_ids()` |
| `session_dir_for` ❌ | `agent_clone_for`, `roster_session_dir` | a free fn over `tddy_data_dir` (M0.3) |
| `mint_first_admission_token` ❌ | `provision_agent_clone` | a free fn over `config` and `session_admissions` (T7, M0.3). `session_admissions` joins the state |
| `split_forward_deadline` ❌ | `provision_agent_clone` (and T4's `start_split_claude_cli_session`) | a free fn over `config` (M0.3). `PEER_FORWARD_TIMEOUT` is already the kernel's. The host method stays: a lifecycle test calls it |
| `resolve_exec_tool_worktree` ❌ | `local_agent_codebase_access` | the existing free fn (`config`, `user_resolver`, `tddy_data_dir`: all in the state), grouped with `resolve_worktree_root_for_session` in T3 |
| `ensure_session_room` ❌ | `ensure_session_room_for_agents`, and T1c's `connect_…` | a T3 function over the state, whose only host need is **`session_room_roster`** |

So the decision reduces to **one new callback** (`session_room_roster`) instead of seven (**D2**).

#### `SplitState` / `SplitHost` (T4, new crate `tddy-session-split`)

| | Content |
|---|---|
| Plan's fields | `config`, `tddy_data_dir`, `session_rooms`, `workspace_sandboxes` |
| Fields the bodies read | plan's four + **`workspace_sandbox_provisioner`**, **`claude_cli_manager`** (`spawn_split_agent_process`), **`session_tokens`** (three sites via `session_tokens()`), **`peer_routing`** (`common_room_slot` at two sites); plus a handle to the agents state (`resolve_specialized_agent_defs`, T3) |
| `SH::start_workspace_session` ✅ (plan) | `start_sandboxed_codebase_session` → `start_session_core` |
| `SH::delete_session` ✅ (plan) | `delete_paired_codebase_session` → `delete_session_at_session_coordinate` |
| `SH::session_room_services` ✅ (plan) = `session_room_roster` | `join_split_livekit_room` |
| `SH::agent_tool_socket` (plan) | **not needed**: `agent_tool_socket_for_embedded_host` is a 4-line function of `tddy_data_dir`, and it moves with T4 |
| `SH::session_files` ❌ **new** | `context_manifest_of`, `context_file_batch_of` (via `session_files_of_this_daemon` → `PeerRoutedSessionFiles`, wiring) |
| `SH::session_agents` ❌ **new** | `split_withdrawals_from_codebase_host` (calls `session_agents_service()`, wiring) |
| `SH::attached_initial_prompt` ❌ **new**, or move it | `spawn_split_agent` calls T1's `attached_initial_prompt`. **Recommended: move it into T4** (T1 → split is the right direction; it needs `AttachmentState` and `stack_doc_attachments`), and no callback |
| remote snapshot | `RemoteCheckout::new(Arc::new(self.clone()))` → AHC `worktree_snapshot` ✅, through `SplitHost: AgentHostCallbacks` |

#### `LaunchState` / `LaunchHost` (T1 + T9 + T1c, new crate `tddy-agent-launch`)

| | Content |
|---|---|
| Fields the bodies read (16) | `agent_activity_hub`, `claude_cli_manager`, `config`, `hosted_agent_clones`, `peer_routing`, `rpc_activity`, `sandbox_manager`, `session_admissions`, `session_agent_inference`, `session_rooms`, `session_stdio`, `spawn_client`, `task_registry`, `tddy_data_dir`, `user_resolver`, `workspace_sandboxes` |
| Handles | the agents handle (AR + AHC), `SplitState` + `SplitHost`, `AttachmentState`, `PresenterObserverDeps` |
| `LH::sandbox_rpc_handler` ✅ (plan) | `launch_jail`, `bridge_relaunched_jail`, `start_sandboxed_cursor_cli_session` |
| `LH::pr_stack` ✅ (plan) | `resolve_chain_base_ref_status`, `record_spawn_on_stack_node` (`rpc_families()?.pr_stack_handler()`) |
| `seed_clone_claimant` | **not a callback** if `DaemonSeedCloneClaimant` holds the agents handle and moves with T3. Otherwise `LH::seed_clone_claimant` ❌ **new** (D3) |
| Owned variant | needed. There are 4 hand-offs: `stream_start_session_at_session_coordinate` (task), `StackChildSpawnHandler`, `GrillMeConversationSpawnHandler` and the host-session socket. `impl StackParentHost` moves onto it |

#### The small states

| State | Fields (read by the bodies) | Callbacks | vs plan |
|---|---|---|---|
| `AttachmentState` (T8) | `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing` | none | exactly as planned. The plan's "calls T7 directly" is really `peer_routing` |
| `AdmissionState` (T7) | only `config` and `session_admissions` | none | **recommend no struct** (D6). The plan's five fields and `OsUserResolver` are not read, and the name collides |
| `DemoVmState` (T11) | `demo_vm_state`, `tddy_data_dir`, `user_resolver`, `rpc_activity`, **`config`** | none | + `config` |
| `PresenterObserverDeps` (T10) | `tddy_data_dir`, `presenter_event_sink`, `session_notification_bus` | none | exactly as planned |

### Cross-topic call matrix

Host-method calls between topics, from the call graph at `9d464a8e`, plus free-function and type
references. **↑** marks an edge that points up the node 17 DAG and must be cut. **W** is wiring.

| Caller ↓ / callee → | T1/T9/T1c | CLI | T4/SU/WS | T3 | T8 | T7 | T10 | T11 | W |
|---|---|---|---|---|---|---|---|---|---|
| **T1/T9/T1c** | — | fields `claude_cli_manager` | 6 calls (`start_sandboxed_codebase_session`, `start_split_claude_cli_session`, `provision_workspace_tool_sandbox` ×2, `resume_split_wiring`, `delete_paired_codebase_session`) | 17 calls (seed, claim, unwind, defs, `ensure_session_room`, `tear_down_every_agent_clone`, `start_hosted_agent_clone`) | `prepare_session_attachments` ×4 | — | `maybe_spawn_presenter_observer` ×2 | — | `sandbox_rpc_handler` ×3 → **LH**; `pr_stack` → **LH**; `seed_clone_claimant`; routing delegations → state |
| **CLI** | — | — | — | — | — | — | — | — | — |
| **T4/SU/WS** | ↑ `start_session_core` → **SH**; ↑ `delete_session_…` → **SH**; ↑ `attached_initial_prompt` → move down; ↑ `write_claude_hooks_settings`, `claude_hook_daemon_url`, `local_daemon_hook_url`, `resolve_start_session_claude_binary` (free, `hooks_and_urls`) → move down (M0.2) | `PtyHandle`, `start_with_options` ✔ (CLI is below) | — | `resolve_specialized_agent_defs` ×2 ✔ | — | — | — | — | `common_room_slot` → state; `session_agents_service`, `session_files_service`, `session_room_roster`, remote snapshot → **SH** |
| **T3** | ↑ `advertise_daemon_url` (free, `hooks_and_urls`) → `daemon_urls` (M0.2) | — | ↑ `split_forward_deadline` → free fn (M0.3); ↑ `peer_has_no_such_session` (free, `split_start`) → T3 (M0.1); ↑ `split_pairing` (free, `split_session`) → T3 (M0.1); ↑ `resolve_worktree_root_for_session` (free, `workspace_session`) → T3 (M0.1) | — | — | `mint_first_admission_token` ✔ (livekit is below) | — | — | `common_room_slot`, `eligible_instance_ids`, `resolve_exec_tool_worktree`, `session_dir_for` → state; `local_exec_tools`, `run_exec_tool_locally`, `session_room_roster` → **AHC** |
| **T8** | — | — | — | — | — | — | — | — | `common_room_slot`, `classify_daemon_route` → state |
| **T10**, **T11**, **T7** | — | — | — | — | — | — | — | — | T11 `record_rpc_activity` → state (`rpc_activity`) |
| **W** | 9 `SessionHandler`/`SessionService` → T1c | — | — | 15 adapter calls → T3 | — | — | — | — | — |

**Are the planned cuts enough? No.** The plan named four cuts. Two are already done or need no
work:
- T1 ↔ T3 needs no work: agent-def resolution is in T3, and no T3 body calls T1 any more (#526's
  `spawn_agent_def`);
- T3 → T2 and T4 → T2 become state reads plus AHC/SH.

The other two hold as planned: T1 ↔ T4 via `SH::start_workspace_session`, and T3 → T4's
`split_forward_deadline`.

The code needs **eight more**:
1. T3 → T4 `peer_has_no_such_session` (the free predicate in `split_start.rs`). This is the
   pilot's refusal shape.
2. T3 → T4 `split_pairing` (`agent_roster.rs:230` → `split_session`).
3. T3 → WS `resolve_worktree_root_for_session`.
4. T3 → T1 `advertise_daemon_url`.
5. T4 → T1 the `hooks_and_urls` helpers.
6. T4 → T1 `attached_initial_prompt`.
7. T4 → T1c `delete_session_at_session_coordinate` (`SH::delete_session`, which the plan had).
8. T4 → WS `PairedAgentSession`. It is solved by putting `workspace_session` in split, not in launch.

The PTY runtime also needs its own crate below split (D4). Otherwise split → launch closes a cycle.

### Receiver budget

The projection is the conversion's topic lines, plus about 1 % for module headers, plus the state and
trait definitions. #526's whole-module moves came in at ~1:1: T5 −960/+968, livekit −455/+454.

| Receiver | Production lines now | Arrives in node 17 | State/port defs (est.) | Projected | ≤ 10k |
|---|---:|---:|---:|---:|---|
| `tddy-session-agents` | 4,132 | 2,054 (+231 if D7 moves `LocalExecTools`) | ~80 | **~6.3k** (~6.5k) | ✅ |
| `tddy-session-split` (new) | 0 | 3,044 (T4 2,485 + SU 293 + WS 266) | ~120 | **~3.2k** | ✅ |
| `tddy-agent-launch` (new) | 0 | 7,168 (T1 5,500 + T9 902 + T1c 766) | ~150 | **~7.3k** | ✅, 2.7k headroom |
| `tddy-cli-sessions` (new, proposed) | 0 | 1,709 | 0 | **~1.7k** | ✅ |
| `tddy-session-files` | 4,716 | 370 | ~25 | **~5.1k** | ✅ |
| `tddy-session-activity` | 2,541 | ~421 (T10 265 + publishing ~70 + `remote_git_pack_execution` 86) | ~15 | **~3.0k** | ✅ |
| `tddy-daemon-livekit` | 5,863 | 213 (T7 58 + `placement` 155) | 0 | **~6.1k** | ✅ |
| `tddy-daemon-kernel` | 3,409 | ~53 (48 + `split_forward_deadline`) | 0 | **~3.5k** | ✅ |
| `tddy-demo-runner` | 156 | 313 | ~20 | **~0.5k** | ✅ |
| `tddy-vm` (the alternative for T11) | 7,233 | 313 | ~20 | ~7.6k | ✅, but not recommended (D12) |

**If D4 is declined and the PTY runtime folds into split,** split becomes ~4.9k (✅). Folding it into
agent-launch instead (~9.0k) is **impossible**: split → launch is a cycle.

### Lifecycle's projected end state (after node 17)

| What stays | Lines |
|---|---:|
| Ports: `svc_activity_ports` 384, session-agents ports 113 + 278 + 277, session-files ports 200 + 358, `svc_session_lifecycle_ports` 166, `svc_terminal_ports` 176, `demo_vm_entry` 8 | 1,960 |
| Builders: `svc_host_builders` 367, `rpc_activity` 8, `handler_state` 114 | 489 |
| Host struct and `mod` declarations: `connection_service.rs` | 490 |
| `lib.rs` 158, `handler` 61, `service` 101, `daemon_rpc_handler` 165, `rpc_families` 66 | 551 |
| Terminal adapter 213 and bridge 59 | 272 |
| Routing delegations and `context_globs` 157, `local_exec_tool_dispatch` 25, `agent_roster` port impls 44, `svc_shut_down_children` 32, `family_proto_bridge` 14, facades (`session_agent_clone` 31, `pr_stack_rpc` 7, `claude_cli_session` 6) | 316 |
| **Wiring as it is today** | **4,078** |
| + `session_notifications` facade residue | ~26 |
| + the conversion's (16a–16e) new wiring: state builders (~90), three callback-trait impls (~110), delegators kept for public or test-called methods (~70), handle plumbing (~30) | ~300 |
| + `LocalExecTools` if D7 keeps it | 231 |
| − `test_util` gated or moved to a testkit | −366 → 0 |
| **Projected end** | **~4.4k** (D7 moves it) to **~4.6k** (D7 keeps it) |

**The ~3.4k target cannot be reached with `PeerRouted*` staying.** The code says so:
- Wiring alone is **4,078 today**, before the conversion's ports.
- #526's floor table counted ports at ~1.5k; they are **1,960**, because `svc_session_lifecycle_ports`
  and `svc_terminal_ports` were not counted.
- It counted the struct at ~0.3k; `connection_service.rs` is **490**.
- It left out 316 lines of routing delegations and facades.

The two further rows of #526's table are what reaches it:
- `PeerRouted*` behind a forwarding port: `PeerRoutedSessionAgents` 278 + `PeerRoutedSessionFiles`
  358 + `PeerRoutedActivity` ~155 ≈ −0.8k → **~3.6k–3.8k**;
- plus the port adapters as receiver-side impls (−277) → **~3.3k–3.5k**.

The developer ruled `PeerRouted*` stays (2026-09-25), so the target is the developer's to restate
(**D13**).

### New crate edges (all for node 17; none is added in 16a–16e)

The cycle check ran over `cargo metadata --offline` at `9d464a8e`: the workspace's normal-dependency
graph, with every edge below added. **The graph stays acyclic.** Each edge was checked individually:
none has its target reaching its source.

| From | To | Status |
|---|---|---|
| `tddy-session-lifecycle` | `tddy-agent-launch`, `tddy-session-split` | **new crates planned in #526** (plan approved); edges need approval with them |
| `tddy-session-lifecycle` | `tddy-cli-sessions` | **needs approval** (new crate, D4) |
| `tddy-agent-launch` | `tddy-session-split`, `tddy-cli-sessions`, `tddy-session-agents`, `tddy-session-files`, `tddy-session-activity`, `tddy-daemon-livekit`, `tddy-daemon-kernel`, `tddy-daemon-sandbox`, `tddy-spawn`, `tddy-host-service` (`HostSessionService`), `tddy-pr-stack` (`LH::pr_stack`'s type), `tddy-core`, `tddy-projects`, `tddy-worktree-service`, `tddy-semantic-index`, `tddy-stdio`, `tddy-sandbox`, `tddy-discovery`, `tddy-workflow-recipes`, `tddy-task`, `tddy-rpc`, `tddy-service` (22) | **needs approval**. All are already lifecycle's, so no binary's graph grows |
| `tddy-session-split` | `tddy-cli-sessions`, `tddy-session-agents`, `tddy-session-files`, `tddy-session-activity`, `tddy-daemon-livekit`, `tddy-daemon-kernel`, `tddy-daemon-sandbox`, `tddy-daemon-auth` (`SessionTokens`), `tddy-github`, `tddy-livekit`, `tddy-core`, `tddy-projects`, `tddy-worktree-service`, `tddy-semantic-index` (`service_util`), `tddy-sandbox`, `tddy-sandbox-recipes`, `tddy-discovery`, `tddy-workflow-recipes`, `tddy-task`, `tddy-rpc`, `tddy-service` (21) | **needs approval** |
| `tddy-cli-sessions` | `tddy-terminal-rpc`, `tddy-session-activity` (`signal_pid`), `tddy-daemon-sandbox` (`SessionScopedResource`), `tddy-stdio`, `tddy-pty`, `tddy-livekit`, `tddy-core`, `tddy-task`, `tddy-rpc`, `tddy-service` (10) | **needs approval** |
| `tddy-session-files` | `tddy-daemon-livekit` | **needs approval** (named in State B for T8, never decided) |
| `tddy-demo-runner` | `tddy-daemon-kernel`, `tddy-core`, `tddy-rpc`, `tddy-service` | **needs approval** (D12) |
| `tddy-session-agents` | `tddy-daemon-sandbox`, `tddy-sandbox-runner`, `tddy-tool-engine`, `tddy-task` | **needs approval, only if D7 moves `LocalExecTools`** |
| `tddy-session-activity` | `tonic` | ✅ approved (2026-09-25, for T10) |
| `tddy-session-agents` | T3 needs **no new edge** once M0.1 moves the three free items down. `tddy-spawn`, the pilot's blocker, is not needed (`ensure_project_available_for_start` is T1) | — |
| `tddy-daemon-livekit`, `tddy-daemon-kernel` | none: `placement`, T7, `agent_list_mapping` and the URL trio name only what they already have | — |

**Module-name collisions** in the receivers:
- the only one is lifecycle's `session_notifications` vs activity's own, avoided by M2's new name;
- none of the modules arriving in agents, files, livekit or kernel shares a name with the ones they
  already have. Checked against each `lib.rs`: agents' `ports`, the kernel's `agent_tool_socket` and
  `peer_forwarding` were the near-misses.

**The `tddy-bsp` note.** Two things:
- `tddy-session-agents` → `tddy-model-registry` (approved) → `tddy-coder` → `tddy-bsp`. So every crate
  above agents (split, agent-launch, cli-sessions only via split's users) carries `tddy-coder` and
  `tddy-bsp` in its graph. Lifecycle already does, so no binary grows. The crates are not light.
- **Lifecycle declares seven normal dependencies that nothing in `src/` or `tests/` names:**
  `tddy-coder`, `tddy-bsp`, `tddy-demo-runner`, `tddy-lsp`, `tddy-lsp-executor`,
  `tddy-screen-sharing`, `tddy-actions`. `tddy-connectrpc` and `tddy-session-sync` are named only by
  tests. Removing them is manifest hygiene, outside a behaviour-preserving restructure. See D14.

## Final design — dependency graph

### Current state (after #526)

```mermaid
graph TD
  subgraph consumers["Consumers"]
    daemon["tddy-daemon"]
    drpc["tddy-daemon-rpc"]
    tgc["tddy-telegram-control"]
  end
  lc["tddy-session-lifecycle - 20,041 lines - host + T1 T3 T4 T8 T9 T10 T11 + wiring"]
  agents["tddy-session-agents 4,132 - AgentRosterState + 11 T3 fns"]
  files["tddy-session-files 4,716"]
  activity["tddy-session-activity 2,541"]
  livekit["tddy-daemon-livekit 5,863"]
  kernel["tddy-daemon-kernel 3,409"]
  sandbox["tddy-daemon-sandbox 3,231"]
  trpc["tddy-terminal-rpc 2,526"]
  demo["tddy-demo-runner 156"]
  vm["tddy-vm"]
  catalog["tddy-session-catalog - unaffected"]
  subgraph found["Foundations"]
    core["tddy-core"]
    projects["tddy-projects"]
    wts["tddy-worktree-service"]
    mreg["tddy-model-registry"]
    coder["tddy-coder / tddy-bsp"]
  end
  daemon --> lc
  drpc --> lc
  tgc --> lc
  lc --> agents
  lc --> files
  lc --> activity
  lc --> livekit
  lc --> kernel
  lc --> sandbox
  lc --> trpc
  lc --> demo
  lc --> vm
  lc --> mreg
  agents --> livekit
  agents --> kernel
  agents --> mreg
  agents --> projects
  activity --> files
  activity --> livekit
  activity --> sandbox
  livekit --> sandbox
  livekit --> kernel
  livekit --> wts
  files --> kernel
  files --> wts
  sandbox --> kernel
  trpc --> kernel
  demo --> vm
  kernel --> core
  mreg --> coder
  coder --> catalog
```

### Final design, after node 17: `tddy-session-lifecycle` as only a wiring node

Edge legend:
- `-->` existing;
- `==>` new and **approved**;
- `-.->` new and **needs approval**;
- `--o` "implements" (lifecycle's host implements a receiver's callback trait).

There is deliberately **no edge into `tddy-session-lifecycle` from any receiver**.

```mermaid
graph TD
  subgraph consumers["Consumers - unedited"]
    daemon["tddy-daemon"]
    drpc["tddy-daemon-rpc"]
    tgc["tddy-telegram-control"]
  end

  subgraph lc["tddy-session-lifecycle - wiring only, about 4.4k to 4.6k"]
    host["DaemonSessionHost - struct, builders, state builders"]
    impls["port impls - AgentHostCallbacks, SplitHost, LaunchHost"]
    routed["PeerRouted* and adapters - SessionService, SessionHandler"]
    term["terminal adapter, bridge, ports"]
    fac["facades - pub use"]
  end

  subgraph al["tddy-agent-launch - new, about 7.3k"]
    al_state["LaunchState + owned launch handle"]
    al_host["trait LaunchHost - sandbox_rpc_handler, pr_stack"]
    al_t1["T1 starts, jails, relaunch, resume - claude_cli_spawn, cursor_cli_spawn"]
    al_t9["T9 stack_parent, child and conversation spawns"]
    al_t1c["T1c session_coordinate_handlers"]
  end

  subgraph split["tddy-session-split - new, about 3.2k"]
    sp_state["SplitState"]
    sp_host["trait SplitHost: AgentHostCallbacks - start_workspace_session, delete_session, session_files, session_agents"]
    sp_t4["T4 split agent, paired teardown, split context, sandboxed codebase"]
    sp_su["service_util, workspace_session"]
  end

  subgraph cli["tddy-cli-sessions - new, proposed, about 1.7k"]
    cli_m["cli_session_manager, session_toolcall"]
  end

  subgraph agents["tddy-session-agents - about 6.3k"]
    ag_state["AgentRosterState + owned handle"]
    ag_cb["trait AgentHostCallbacks - local_exec_tools, run_exec_tool_locally, worktree_snapshot, session_room_roster"]
    ag_t3["T3 roster, clones, agent defs, peer_session_answer, split_pairing"]
  end

  subgraph files["tddy-session-files - about 5.1k"]
    fi_state["AttachmentState"]
    fi_t8["T8 staged and session attachments"]
  end

  subgraph activity["tddy-session-activity - about 3.0k"]
    ac_deps["PresenterObserverDeps"]
    ac_t10["T10 presenter observer and intent client, session_notification_publishing"]
    ac_leaf["remote_git_pack_execution"]
  end

  subgraph livekit["tddy-daemon-livekit - about 6.1k"]
    lk_t7["T7 admission token, resolve_os_user"]
    lk_pl["placement"]
  end

  subgraph kernel["tddy-daemon-kernel - about 3.5k"]
    kn["agent_list_mapping, daemon_urls, split_forward_deadline"]
  end

  subgraph demo["tddy-demo-runner - about 0.5k"]
    dm_state["DemoVmState"]
    dm_t11["T11 demo VM handlers, DemoVmServiceImpl"]
  end

  sandbox["tddy-daemon-sandbox"]
  trpc["tddy-terminal-rpc"]
  vm["tddy-vm"]
  catalog["tddy-session-catalog - unaffected"]
  tonic["tonic - external"]

  subgraph found["Foundations"]
    core["tddy-core, tddy-rpc, tddy-service"]
    projects["tddy-projects, tddy-worktree-service"]
    mreg["tddy-model-registry"]
    coder["tddy-coder, tddy-bsp"]
    misc["tddy-spawn, tddy-semantic-index, tddy-stdio, tddy-pty, tddy-pr-stack, tddy-host-service, tddy-daemon-auth"]
  end

  daemon --> lc
  drpc --> lc
  tgc --> lc

  lc -.-> al
  lc -.-> split
  lc -.-> cli
  lc --> agents
  lc --> files
  lc --> activity
  lc --> livekit
  lc --> kernel
  lc --> demo
  lc --> sandbox
  lc --> trpc

  impls --o al_host
  impls --o sp_host
  impls --o ag_cb

  al -.-> split
  al -.-> cli
  al -.-> agents
  al -.-> files
  al -.-> activity
  al -.-> livekit
  al -.-> kernel
  al -.-> sandbox
  al -.-> misc

  split -.-> cli
  split -.-> agents
  split -.-> files
  split -.-> activity
  split -.-> livekit
  split -.-> sandbox
  split -.-> misc

  cli -.-> trpc
  cli -.-> activity
  cli -.-> sandbox

  agents --> livekit
  agents --> kernel
  agents --> mreg
  agents --> projects

  files -.-> livekit
  files --> kernel
  files --> projects

  activity --> files
  activity --> livekit
  activity --> sandbox
  activity ==> tonic

  livekit --> sandbox
  livekit --> kernel
  livekit --> projects

  demo --> vm
  demo -.-> kernel

  sandbox --> kernel
  trpc --> kernel
  kernel --> core
  mreg --> coder
  coder --> catalog
```

`activity ==> tonic` is the one approved new edge, and `tonic` is external. Lines inside the boxes are
the budget projections above.

**Acyclic.** `cargo metadata` plus every edge below, checked as described in "New crate edges".
Every receiver's topological position is below lifecycle, and within the receivers:
`agent-launch` > `split` > `cli-sessions`; `split` > `agents`; and `agents`, `files` and `activity` >
`livekit` > `sandbox` > `kernel` > `core`.

| From | To | Status |
|---|---|---|
| `tddy-daemon`, `tddy-daemon-rpc`, `tddy-telegram-control` | `tddy-session-lifecycle` | existing |
| `tddy-session-lifecycle` | agents, files, activity, livekit, kernel, sandbox, terminal-rpc, demo-runner | existing |
| `tddy-session-lifecycle` | `tddy-agent-launch`, `tddy-session-split` | new; crates planned in #526, edges need approval |
| `tddy-session-lifecycle` | `tddy-cli-sessions` | new, needs approval (D4) |
| `tddy-agent-launch` | the 22 listed under "New crate edges" | new, needs approval |
| `tddy-session-split` | the 21 listed under "New crate edges" | new, needs approval |
| `tddy-cli-sessions` | the 10 listed under "New crate edges" | new, needs approval |
| `tddy-session-agents` | livekit, kernel, model-registry, projects, core, worktree-service | existing (model-registry and projects approved and added in #526) |
| `tddy-session-agents` | sandbox, sandbox-runner, tool-engine, task | new, needs approval, **only under D7-B** |
| `tddy-session-files` | kernel, core, worktree-service, workflow, sandbox | existing |
| `tddy-session-files` | `tddy-daemon-livekit` | new, needs approval |
| `tddy-session-activity` | files, livekit, sandbox, kernel, core, projects, worktree-service, telegram | existing (added in #526) |
| `tddy-session-activity` | `tonic` (external) | new, approved (2026-09-25) |
| `tddy-daemon-livekit` | sandbox, kernel, host-service, worktree-service, core | existing |
| `tddy-daemon-kernel` | core, discovery, sandbox, livekit, rpc, service, task, github | existing (task and github approved in #526) |
| `tddy-demo-runner` | `tddy-vm`, `tddy-workflow-recipes` | existing |
| `tddy-demo-runner` | kernel, core, rpc, service | new, needs approval (D12) |
| `tddy-terminal-rpc` | `tddy-daemon-kernel` | existing (added in #526) |
| `tddy-model-registry` | `tddy-coder` (→ `tddy-bsp`) | existing |
| any receiver | `tddy-session-lifecycle` | **must not exist**, normal or dev |

## Acceptance graph — after this node

After 16a there are no crate moves. This is **lifecycle's module layout after 16a**:
- the four leaf topics converted (T7, T8, T10, T11), each over its state, none with a callback;
- the host-free leaves and the three modules M0 creates (`peer_session_answer`, `daemon_urls`,
  `session_notification_publishing`);
- the topics **not converted yet**, still host methods, shown as one box each;
- the edges that must not exist.

The stack's end state for the in-place conversion (after 16e) is kept below it, under "Conversion end
state (after 16e)", because it is the master plan every successor node's graph is a slice of.

Edge legend:
- `-->` direct call (allowed);
- `-.->` a call through a state (the host builds it);
- `--x` **must not exist** (acceptance).

```mermaid
graph TD
  subgraph lc["tddy-session-lifecycle after carve 16a - about 20.1k, nothing moved"]
    subgraph W["wiring - stays"]
      host["DaemonSessionHost - connection_service.rs, svc_host_builders"]
      builders["state builders in handler_state.rs - DemoVmState, PresenterObserverDeps, AttachmentState"]
      dele["delegators - mint_first_admission_token, split_forward_deadline, session_dir_for"]
      demo_entry["demo_vm_entry"]
    end

    subgraph DONE["converted in 16a"]
      t7["T7 - first_admission_token fn, resolve_os_user fn"]
      t8["T8 - AttachmentState - svc_materialize_staged_attachment, session_attachment_materialization"]
      t10["T10 - PresenterObserverDeps - presenter_observer_spawn, presenter_observer_task, presenter_intent_client"]
      t10p["session_notification_publishing"]
      t11["T11 - DemoVmState - demo_vm_coordinate_handlers, activity_hub, DemoVmServiceImpl"]
      leaves["leaves - placement, remote_git_pack_execution, agent_list_mapping, daemon_urls"]
    end

    subgraph TODO["not converted yet - host methods"]
      t3["T3 agents - 16b - now owns peer_session_answer"]
      t4["T4, service_util, workspace_session - 16c"]
      cli["CLI PTY runtime - imports in 16c"]
      t1["T1, T9, T1c launch - 16d and 16e"]
    end
  end

  builders -.-> t8
  builders -.-> t10
  builders -.-> t11
  demo_entry -.-> t11
  dele --> t7
  t10 --> t10p
  t3 --> t7
  t3 --> leaves
  t1 -.-> t8
  t1 -.-> t10
  t4 -.-> t8

  t7 --x host
  t8 --x host
  t10 --x host
  t11 --x host
  leaves --x host
  t8 --x t4
  t8 --x t3
  t10 --x t1
  t11 --x t1
  t3 --x t4
  t3 --x t1
  t4 --x t1
```

`t3 --x t4` and `t3 --x t1` mean the three free items and `advertise_daemon_url`: T3's host methods
still call T4's `split_forward_deadline` through its delegator, which is wiring. `t4 --x t1` is the
four `hooks_and_urls` helpers (M0.2); the two `SplitHost` cuts and the `attached_initial_prompt` move
are 16c's. `t8 --x t4` is the re-parented `split_claude_cli_start` (D8).

### Acceptance criteria for 16a, and how each is checked

The **16a topic files** are: T7 (`svc_host_builders/first_admission_token.rs` and the `resolve_os_user`
module), T8 (`svc_materialize_staged_attachment.rs`, `session_attachment_materialization.rs`), T10
(`presenter_observer_spawn.rs`, `presenter_observer_task.rs`, `presenter_intent_client.rs`,
`session_notification_publishing.rs`), T11 (`demo_vm_coordinate_handlers.rs`, `activity_hub.rs`,
`svc_demo_vm_ports.rs` outside `demo_vm_entry`) and the leaves (`placement.rs`,
`remote_git_pack_execution.rs`, `agent_list_mapping.rs`, `daemon_urls.rs`). A shape test is possible
for every row, but by default each is a grep (D11).

| # | Criterion: what must **not** exist | How to check |
|---|---|---|
| A1 | No 16a topic file names `DaemonSessionHost`, as a type, an `impl` or a method call. The M0.3 delegators live in a wiring file | `grep -n 'DaemonSessionHost' <16a topic files> \| grep -v '^\S*:\s*//'` is empty (for `svc_demo_vm_ports.rs`, outside `demo_vm_entry`) |
| A2 | No 16a topic file names a wiring module: `connection_service`'s own items, `svc_*_ports`, `handler_state`, `svc_host_builders`, `rpc_families`, `PeerRouted*`, `DaemonRpcHandler`, or `test_util` outside `#[cfg(test)]` | a grep of each 16a topic file for `super::(super::)?(DaemonRpcHandler\|PeerRouted\|handler_state\|svc_host_builders\|…)`, `crate::rpc_families` and `crate::test_util` outside test modules; empty |
| A3 | No upward edge from a converted topic: T7, T8, T10, T11 and the leaves ↛ any other topic (T10 → `session_notification_publishing` is allowed). **Plus the six M0 cuts:** no T3 file names `split_start::peer_has_no_such_session`, `split_session::split_pairing`, `workspace_session::resolve_worktree_root_for_session` or `hooks_and_urls::advertise_daemon_url`; no T4 file names a `hooks_and_urls` item | a scripted grep: for each 16a topic file, collect `crate::…`, `super::…` and `crate::connection_service::…` module targets, map each to its topic by the inventory, and fail on any pair outside the allowed DAG; plus the six named greps, each empty. Optionally `cargo modules dependencies --lib -p tddy-session-lifecycle` |
| A3b | The four re-parented children (D8) have no parent from another topic, and the D9 test is out | `grep -n 'mod split_claude_cli_start' svc_materialize_staged_attachment.rs`, `grep -n 'mod session_attachment_materialization' svc_resolve_os_user.rs`, `grep -n 'mod jail_env_builders' svc_turn_end_reporter.rs`, `grep -n 'mod svc_host_builders' svc_resolve_tddy_tools_path.rs` are empty; `grep -n 'DaemonSessionHost' svc_split_context_from_codebase_host.rs` has no hit inside a `#[cfg(test)]` module |
| A4 | 16a topic files name foundations and receivers **by their defining crate**, never through a lifecycle facade | `grep -nE 'crate::(config\|relay_idle\|livekit_peer_discovery\|session_room\|peer_routing\|session_admission_service\|context_files\|context_sync\|session_attachments\|session_reader\|session_deletion\|user_sessions_path\|session_agent_[a-z]+\|project_storage\|branch_intent\|pty_runtime\|host_session_service)\b' <16a topic files>` is empty |
| A5 | No 16a topic file clones the host | follows from A1, plus `grep -n 'Arc::new(self.clone())'` in 16a topic files is empty |
| A6 | No callback trait exists yet, and the 16a states carry none: T7, T8, T10 and T11 take a state only (no `AdmissionState`, D6) | `grep -rn 'trait AgentHostCallbacks\|trait SplitHost\|trait LaunchHost' packages/tddy-session-lifecycle/src packages/tddy-session-agents/src` is empty; `grep -rn 'struct AdmissionState'` is empty |
| A7 | The public API is unchanged: no consumer edit, every facade still resolves, and `tddy-session-agents` is untouched | `git diff <base> -- packages/tddy-daemon-rpc packages/tddy-daemon packages/tddy-telegram-control packages/tddy-desktop packages/tddy-session-agents` is empty, and `cargo check --all-targets` is clean on lifecycle, `tddy-session-agents`, `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` (`tddy-desktop` on CI: it embeds the web bundle) |
| A8 | Behaviour: the baseline | 562 passed, the same 22 by name, 1 ignored, after M0, M1, M2 and M3. `restructure verify --against <base>` accounted |

### Conversion end state (after 16e)

This is where 16a–16e end together, and the graph node 17 starts from. Each successor node's
acceptance graph is a slice of it.

Edge legend:
- `-->` direct call (allowed);
- `-.->` a call through a port or state;
- `--o` implements;
- `--x` **must not exist** (acceptance).

```mermaid
graph TD
  subgraph lc["tddy-session-lifecycle after carve 16e - about 20.3k, nothing moved yet"]
    subgraph W["wiring - stays"]
      host["DaemonSessionHost - connection_service.rs, svc_host_builders"]
      builders["state builders - handler_state.rs"]
      impls["port impls - one file - impl AgentHostCallbacks, SplitHost, LaunchHost"]
      routed["PeerRouted*, adapters, SessionHandler and SessionService impls"]
    end

    subgraph L["launch topic - T1, T9, T1c"]
      l_ports["launch_ports: LaunchState, owned launch handle, trait LaunchHost"]
      l_t1["T1 modules: svc_start_*, claude_cli_spawn, cursor_cli_spawn, relaunch, managed_launch, hooks_and_urls"]
      l_t9["T9 modules: stack_parent, child and - conversation spawn, svc_pr_status_for_caller"]
      l_t1c["T1c modules: session_coordinate_handlers"]
    end

    subgraph S["split topic - T4, service_util, workspace_session"]
      s_ports["split_ports: SplitState, trait SplitHost"]
      s_t4["T4 modules: svc_spawn_split_agent, split_session, split_start, split context, sandboxed codebase"]
      s_su["service_util, workspace_session"]
    end

    subgraph C["PTY runtime - CLI"]
      c_m["cli_session_manager, session_toolcall"]
    end

    subgraph A["agents topic - T3"]
      a_ports["agent_host_callbacks: trait AgentHostCallbacks, owned roster handle"]
      a_t3["T3 modules: agent_roster, svc_provision_agent_clone, svc_start_hosted_agent_clone, svc_ensure_session_room_for_agents, svc_resolve_listed_worktree, svc_turn_end_reporter, peer_session_answer"]
    end

    t8["T8: AttachmentState, svc_materialize_staged_attachment"]
    t7["T7: admission token fn, resolve_os_user fn"]
    t10["T10: PresenterObserverDeps, presenter_*, session_notification_publishing"]
    t11["T11: DemoVmState, demo_vm_coordinate_handlers"]
    leaves["leaves: placement, remote_git_pack_execution, agent_list_mapping, daemon_urls"]
  end

  ars["tddy_session_agents::AgentRosterState - exists, gains 2 fields"]

  impls --o l_ports
  impls --o s_ports
  impls --o a_ports
  builders -.-> l_ports
  builders -.-> s_ports
  builders -.-> ars
  routed --> l_t1c
  routed --> a_t3

  l_t1 -.-> l_ports
  l_t9 -.-> l_ports
  l_t1c --> l_t1
  l_t1 --> l_t9
  l_t1 --> s_t4
  l_t1 --> a_t3
  l_t1 --> t8
  l_t1 --> t10
  l_t1 --> c_m
  l_t1 --> s_su
  l_t1c --> a_t3
  l_t1c --> s_t4

  s_t4 -.-> s_ports
  s_t4 --> a_t3
  s_t4 --> c_m
  s_t4 --> s_su
  s_t4 --> t8

  a_t3 -.-> a_ports
  a_t3 -.-> ars
  a_t3 --> t7
  a_t3 --> leaves

  s_t4 --x l_t1
  a_t3 --x s_t4
  a_t3 --x l_t1
  c_m --x l_t1
  l_t1 --x host
  s_t4 --x host
  a_t3 --x host
  t8 --x host
  t11 --x host
```

### Acceptance criteria for the whole conversion (16a–16e), and how each is checked

These are the criteria for the whole conversion, checked in full at 16e. Each node 16a–16e checks them
restricted to the topics it has converted so far (its own changeset lists the restricted set). The
topic file sets are the per-file inventory's groups.

| # | Criterion: what must **not** exist | How to check |
|---|---|---|
| A1 | No topic module names `DaemonSessionHost`, whether as a type, an `impl` or a method call. Delegators live only in wiring files | `grep -n 'DaemonSessionHost' <topic files> \| grep -v '^\S*:\s*//'` is empty |
| A2 | No topic module names a wiring module: `connection_service`'s own items, `svc_*_ports`, `handler_state`, `svc_host_builders`, `rpc_families`, `PeerRouted*`, `DaemonRpcHandler`, or `test_util` outside `#[cfg(test)]` | a grep of each topic file for `super::(super::)?(DaemonRpcHandler\|PeerRouted\|handler_state\|svc_host_builders\|…)`, `crate::rpc_families` and `crate::test_util` outside test modules; empty |
| A3 | No upward topic edge:<br>• T3 ↛ T4/SU/WS/T1/T9/T1c/CLI;<br>• T4/SU/WS ↛ T1/T9/T1c;<br>• CLI ↛ any topic;<br>• T7, T8, T10, T11 and the leaves ↛ any other topic (T10 → its publishing module is allowed) | a scripted grep: for each topic's files, collect `crate::…`, `super::…` and `crate::connection_service::…` module targets, map each to its topic by the inventory, and fail on any pair outside the allowed DAG. Optionally `cargo modules dependencies --lib -p tddy-session-lifecycle`, if installed |
| A4 | Topic modules name foundations and receivers **by their defining crate** (`tddy_daemon_kernel::config::…`, `tddy_daemon_livekit::session_room::…`), never through a lifecycle facade (`crate::config`, `crate::session_room`, `crate::user_sessions_path`, …). A lower topic is named by its own module path, never through a parent's re-export glob | `grep -nE 'crate::(config\|relay_idle\|livekit_peer_discovery\|session_room\|peer_routing\|session_admission_service\|context_files\|context_sync\|session_attachments\|session_reader\|session_deletion\|user_sessions_path\|session_agent_[a-z]+\|project_storage\|branch_intent\|pty_runtime\|host_session_service)\b' <topic files>` is empty. This is what keeps node 17 clear of the import-reaching-the-destination refusal |
| A5 | No topic module clones the host: no `self.clone()` or `Arc::clone(self)` whose value is a `DaemonSessionHost`. Hand-offs clone the topic's owned handle | follows from A1, plus `grep -n 'Arc::new(self.clone())'` in topic files is empty |
| A6 | Callback traits hold only approved methods. Each trait is defined once, in its topic module, and implemented once, on `DaemonSessionHost`, in a wiring file | `grep -rn 'trait AgentHostCallbacks\|trait SplitHost\|trait LaunchHost'` gives one hit each. The method lists match "State structs and callback traits" as decided. `grep -rn 'impl .*\(AgentHostCallbacks\|SplitHost\|LaunchHost\) for DaemonSessionHost'` gives one hit each, all in wiring |
| A7 | The public API is unchanged: no consumer edit, and every facade still resolves | `git diff <base> -- packages/tddy-daemon-rpc packages/tddy-daemon packages/tddy-telegram-control packages/tddy-desktop` is empty, and `cargo check --all-targets` is clean on the first three plus lifecycle and `tddy-session-agents` |
| A8 | Behaviour: the baseline | 562 passed, the same 22 by name, 1 ignored, after every milestone. `restructure verify --against <base>` accounted |

## Planned: node 17 acceptance graph

Carried into node 17's own changeset (`2026-09-26-carve-lifecycle-moves.md`), where it is
checked. Kept here as part of the master plan. This is the crate-level view of "Final design" above.

```mermaid
graph TD
  daemon["tddy-daemon"] --> lc
  drpc["tddy-daemon-rpc"] --> lc
  tgc["tddy-telegram-control"] --> lc
  lc["tddy-session-lifecycle - wiring only"]
  lc --> al["tddy-agent-launch"]
  lc --> sp["tddy-session-split"]
  lc --> cs["tddy-cli-sessions"]
  lc --> ag["tddy-session-agents"]
  lc --> fi["tddy-session-files"]
  lc --> ac["tddy-session-activity"]
  lc --> lk["tddy-daemon-livekit"]
  lc --> kn["tddy-daemon-kernel"]
  lc --> dm["tddy-demo-runner"]
  lc --o|implements LaunchHost| al
  lc --o|implements SplitHost| sp
  lc --o|implements AgentHostCallbacks| ag
  al --> sp
  al --> cs
  al --> ag
  al --> fi
  al --> ac
  sp --> cs
  sp --> ag
  sp --> fi
  sp --> ac
  cs --> ac
  ag --> lk
  fi --> lk
  ac --> fi
  ac --> lk
  lk --> kn
  dm --> kn
  al --x|must not| lc
  sp --x|must not| lc
  cs --x|must not| lc
  ag --x|must not| lc
  sp --x|must not| al
  ag --x|must not| sp
  cs --x|must not| sp
```

| # | node 17 acceptance criterion | How to check |
|---|---|---|
| B1 | No receiver depends on lifecycle, normal or dev | `cargo tree -i tddy-session-lifecycle -e normal,dev --workspace` lists only `tddy-daemon`, `tddy-daemon-rpc`, `tddy-desktop`, `tddy-telegram-control` (and the existing dev users `tddy-model-registry`, `tddy-tool-engine`, `tddy-worktree-service`) |
| B2 | The edge set is the approved table, and nothing more | the new crates' `Cargo.toml` diffed against "New crate edges" |
| B3 | No reverse edge between receivers: split ↛ agent-launch; agents ↛ split / agent-launch / cli-sessions; cli-sessions ↛ split / agent-launch / agents; files ↛ agents / split / agent-launch | `cargo tree -p <receiver> -e normal,dev` per receiver, grepped for the forbidden names |
| B4 | Every receiver ≤ 10k production lines, and no file ≥ 500 | the counter in "How lines were counted", run per crate |
| B5 | Lifecycle meets the wiring definition (no function beyond construction, delegation and port impls) at the size the developer restates in D13 | counter, plus a review of the remaining files against the "stays" table |
| B6 | Every public `tddy_session_lifecycle::…` path resolves, and consumers are unedited | `git diff` on the three consumers is empty; `cargo check --all-targets` is clean on them |
| B7 | Baseline, moved tests accounted | lifecycle's count falls by exactly the tests that moved, and each receiver's rises by the same, same names |

## Technical changes

### State A

At `9d464a8e`:
- `tddy-session-lifecycle` holds 20,041 production lines in 110 production files;
- **202 `impl DaemonSessionHost` methods**, of which 52 are `pub`;
- the host struct has 34 fields.

Host methods called from outside the crate:
- by consumers: `agent_clone_divergences`, `agent_clone_worktree_path`, `agent_def_for_spawn`,
  `resolvable_agent_defs`, `session_room_participant_identities`, `demo_vm_entry`, plus the
  accessors, builders and service entries;
- by lifecycle's own tests: `split_forward_deadline`, `sandbox_sessions`, `rpc_families`;
- by in-`src` unit tests: `local_agent_codebase_access`, `resolve_specialized_agent_defs`,
  `seeded_roster_records` and `specialized_subagent_env`, all four `pub(crate)`.

### State B (after this node)

**After 16a:** the same crate, about **20.1k** production lines: +~60 of builders, the three small
states and the delegators, all still inside lifecycle. It is measured at wrap. The layout is the 16a
acceptance graph above. The topic files keep their paths, except the four re-parented with consent
(D8) and the new modules M0 and M2 create:
- `peer_session_answer`, T3's group for `peer_has_no_such_session`, `split_pairing`,
  `resolve_worktree_root_for_session` and `resolve_exec_tool_worktree`;
- `daemon_urls`;
- `session_notification_publishing`;
- `connection_service/split_context_from_codebase_host_tests.rs` (D9).

**After 16e** (the end of the in-place conversion): about **20.3k–20.4k** production lines, +~300 of
state, traits, impls and handle plumbing, plus the three ports modules (`agent_host_callbacks`,
`split_ports`, `launch_ports`). The layout is "Conversion end state (after 16e)".

### Conversion recipe (per milestone)

1. **Define the topic's state** (borrowed, plus an owned variant or handle where a hand-off needs
   `'static`) **and its callback trait**, in a ports module of that topic. They are hand-written, as
   #526's "hand-written ports" decision allows.
2. **Implement the trait on `DaemonSessionHost`** in the wiring ports file, and **add the builder**
   to `handler_state.rs`.
3. **Convert each method.** Try the engine first (`extract_method` over the `self`-free tail, then
   `extract_variable` hoists). Otherwise convert by hand under Boundaries' edit rule:
   - `self.<field>` → `state.<field>`;
   - a routing delegation → `state.peer_routing.<m>(…)`;
   - a cross-topic host call → the topic function with its state;
   - a wiring capability → `host.<callback>(…)`;
   - `self.clone()` into a task → a handle clone.
4. **Re-point wiring callers** (adapters, `SessionHandler`) to the topic function. Keep a
   `DaemonSessionHost` delegator only where a consumer or a test calls the method.
5. **Re-point imports** to defining crates (A4).
6. **Gates:** `cargo check --all-targets` on the five crates, clippy and fmt on lifecycle, the
   baseline, A1–A6 for the topics converted so far, and `verify`.

### Milestones and the PR cut

Leaves first, one topic per milestone. **The PR cut is decided (D15, 2026-09-26):**

| Milestone | Topic | Lines converted or touched | Depends on | Node |
|---|---|---:|---|---|
| M0 | baseline, the six port-free cuts, re-parenting, test relocation, `seeded_clone_guard` split | ~300 | — | **16a** |
| M1 | T11 demo VM | 321 | M0 | **16a** |
| M2 | T10 presenter + `session_notifications` split | 361 | M0 | **16a** |
| M3 | T7 + T8 | 428 | M0 | **16a** |
| M4 | T3 agents | ~2,100 (+ adapters) | M0, M3 (admission fn) | 16b |
| M5 | T4 + `service_util` + `workspace_session` (+ CLI imports) | ~3,050 (+1,709 imports only) | M4 (defs through AR), M3 (AS) | 16c |
| M6 | T9 stack spawns | 902 | M3 | 16d |
| M7a | T1, the jail and CLI-spawn half | 3,742 (of which 1,532 imports only) | M4, M5, M6 | 16d |
| M7b | T1, the start/resume half | 1,755 | M7a, M2 | 16e |
| M8 | T1c coordinate handlers | 766 | M7b | 16e |

**How M7 is split.** The source plan named the halves ("the jail and CLI-spawn half", "the start/resume
half") without a file list. The split below is defined from the per-file inventory and **checked
against the call graph at `9d464a8e`**: no M7a body calls an M7b host method, so 16d can pass A1 for
its own files while the M7b methods are still host methods.

- **M7a (16d), 3,742 lines**:
  - the jail starts: `svc_start_sandboxed_claude_cli_session.rs` (493) and its `jail_launch_steps`
    (221), `jail_session_files` (104), `jail_worktree` (136); `svc_start_sandboxed_cursor_cli_session.rs`
    (445); `svc_relaunch_sandboxed_runner.rs` (213) with `relaunch_jail_dirs` (73) and
    `relaunch_jail_steps` (229); `svc_turn_end_reporter/jail_env_builders.rs` (71, re-parented in
    M0.4); `svc_split_context_from_codebase_host.rs`'s T1 part, `resume_sandboxed_claude_cli_session`
    (62, it calls `relaunch_sandboxed_runner`);
  - the CLI spawns: `svc_start_claude_cli_session.rs` (236); `claude_cli_spawn.rs` (285),
    `claude_cli_spawn_steps.rs` (300), `cursor_cli_spawn.rs` (445), `cursor_cli_spawn/chat.rs` (94),
    `cursor_cli_spawn/resume.rs` (96), all free (imports only);
  - pulled in by the call graph: `svc_pr_status_for_caller.rs`'s T1 part (104:
    `prepare_managed_workflow`, `managed_resume_goal`, `owned_branch_conflict`), because the jail
    starts call `prepare_managed_workflow` and the file's T9 part is M6; `managed_launch.rs` (95, free,
    `prepare_managed_workflow_inner`) and `worktree_source.rs` (40, free,
    `sandbox_claude_passthrough_args`) with it; and `svc_start_claude_cli_session.rs`, because the jail
    starts call its `conversation_spawn_handler_for`.
- **M7b (16e), 1,755 lines**: `svc_start_session_core.rs` (455) and its `cli_branch_starts` (220),
  `start_request_checks` (98), `tool_session_spawn` (228), `tool_spawn_plan` (67, free),
  `workspace_branch_start` (122); `svc_resume_claude_cli_session.rs`'s T1 part (172);
  `hooks_and_urls.rs`'s T1 part (~186, free: `effective_spawn_branch`, `claude_cli_participant_metadata`,
  `spawned_branch_of_session`, `resolve_resume_session_claude_binary`);
  `svc_resolve_listed_worktree.rs`'s T1 part (185, `ensure_project_available_for_start`,
  `spawn_project_clone`, extracted into a T1 module); `svc_ensure_session_room_for_agents.rs`'s T1
  part (22, `index_workspace_worktree`).

M7a + M7b = 5,497, the inventory's T1 total (5,500). **16d is larger than the source's estimate**
(~4.6k touched against "~3.5k"): 1.7k of it (T9 and M7a) is free files whose only edit is imports (A4), so its
converted diff is ~2.9k. 16e is ~2.5k against the estimate's ~2.9k.

**`LaunchState` is split with M7.** 16d defines `LaunchState`, the owned launch handle and
`LaunchHost` (both methods: M7a's jails call `sandbox_rpc_handler`, M6's T9 calls `pr_stack`). Its
state carries the seven fields 16d's bodies read (`config`, `tddy_data_dir`, `claude_cli_manager`,
`task_registry`, `sandbox_manager`, `session_stdio`, `agent_activity_hub`, grepped at `9d464a8e`); 16e
adds the other nine of the sixteen. `LaunchHost` is not changed by 16e.

Each node holds the baseline on its own, and each one's acceptance is A1–A8 restricted to the topics
converted so far. node 17 is one node (decided); it could be cut the same way, one receiver per
node, later. That cut is cheaper, since moves review faster than conversions.

### Callers

| | |
|---|---|
| External importers | none change. No path moves here |
| Facade | none added. Existing `pub use` lines are untouched |
| Consequence | no consumer file in the diff (A7). Lifecycle's own in-`src` unit tests keep compiling through the delegators kept for the four `pub(crate)` methods they call, or are re-pointed mechanically to the topic function (listed per milestone) |

### Visibility

This node widens nothing across a crate. Within lifecycle, a topic function that another topic calls
may need `pub(crate)` where the host method was `pub(in crate::connection_service)` or `pub(super)`.
Each widening is recorded per milestone in the restructure reference's table. node 17 does the
`pub(crate)` → `pub` widening that crossing a crate requires.

### Baseline

Recorded before M0, and the acceptance criterion after every milestone:

```bash
./dev cargo test -p tddy-session-lifecycle --no-fail-fast -- --test-threads=1 \
  --skip sandboxed_bash_pty_action_streams_output
```

Expected: **562 passed, 22 failed, 1 ignored**, the same 22 by name. The flaky
`session_room_acceptance::the_first_connect_makes_the_sessions_terminal_drivable_over_livekit`
passes when re-run alone, so it is not a regression.

| Suite | Known red (macOS) | Cause |
|---|---|---|
| `sandbox_behavior_acceptance` (5) | `sandboxed_session_streams_demo_tui_dimensions_in_terminal`, `sandboxed_session_spawn_manifest_records_session_channel_egress`, `sandboxed_session_relays_claude_llm_egress_via_session_channel`, `sandboxed_session_denies_direct_outbound_network_from_jail`, `sandboxed_session_child_is_alive_after_demo_tui_start` | `sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge` |
| `sandboxed_claude_cli_acceptance` (5) | `sandboxed_claude_cli_start_persists_metadata_and_empty_livekit`, `sandboxed_claude_cli_connect_session_returns_empty_livekit`, `sandboxed_claude_cli_terminal_io_round_trips`, `sandboxed_claude_cli_tool_exec_via_ipc_reads_host_worktree`, `sandboxed_claude_cli_start_wires_specialized_agents_env_and_metadata` | same |
| `sandboxed_cursor_cli_acceptance` (4) | `sandboxed_cursor_cli_start_persists_metadata_and_empty_livekit`, `sandboxed_cursor_cli_connect_session_returns_empty_livekit`, `sandboxed_cursor_cli_terminal_io_round_trips`, `sandboxed_cursor_cli_start_wires_specialized_agents_env_and_metadata` | same |
| `sandboxed_session_lifecycle_acceptance` (2) | `delete_sandbox_session_stops_child_and_removes_directory`, `resume_sandbox_session_respawns_and_updates_pid` | same |
| `session_sync_livekit_acceptance` (6) | `mirrors_a_file_the_agent_wrote_without_a_commit`, `mirrors_an_edit_to_an_existing_file`, `removes_a_file_the_agent_deleted`, `mirrors_binary_content_byte_for_byte`, `follows_the_session_head_when_the_agent_commits`, `restores_a_mirror_that_was_corrupted_by_hand` | `tddy-remote-git-repo is not built`, then `Once instance has previously been poisoned` |

Also run after every milestone: `./dev cargo test -p tddy-session-agents` (72 passed on #526's base)
once M4 widens `AgentRosterState`.

**What the baseline cannot guard.** The 16 sandboxed failures above never execute the sandboxed
start, relaunch and delete paths on macOS. Those paths are converted in M4, M5 and M7, so their
preservation there is argued from:
- compiling;
- the edit rule;
- **Linux CI's run** of the same suites (`scripts/ci-status.sh --failures` on the PR).

## Decisions & trade-offs

Settled by the developer (2026-09-26), carried here:
- **D15: the cut.** Five linear conversion nodes (16a–16e, see "Milestones and the PR cut") plus
  one move node (node 17), decided 2026-09-26. node 17 stays one node; cutting it one receiver
  per node later is noted in its changeset.
- Two nodes. This one converts in place, and node 17 moves.
- Engine moves only across crates, and a refusal means stop and ask.
- New edges need approval. No receiver may depend on lifecycle.
- Each crate stays at or under 10k. Consumers stay unedited. Facades are kept.
- `AgentHostCallbacks` = {`worktree_snapshot`, `run_exec_tool_locally`, `local_exec_tools`} is
  approved. The seven further methods are not.
- `PeerRouted*` stays.
- The ~3.4k target.

### Open decisions for the developer

- **D1: how the conversion is shaped, and the `self.clone()`-to-task hand-offs.** Twelve methods hand
  the whole host to something `'static`:
  - T3: `claim_agent_clone`, `claim_co_located_seed_clones`, `owned_agent_codebase_access`,
    `local_agent_codebase_access`, `ensure_session_room`;
  - T4: `join_split_livekit_room` ×2, `session_files_of_this_daemon`;
  - T1: three spawn handlers and the host-session socket;
  - T1c: `stream_start_session`.

  Two options:
  - **Recipe A (as the developer phrased it):** free functions over a borrowed state
    (`AgentRosterState<'a>`, the pilot's shape), plus an owned handle (Arc clones, a `DaemonConfig`
    clone and `Arc<dyn …Callbacks>`) for hand-offs. It adds `state` and `host` parameters to every
    function, and it **worsens two code issues** (`resume_claude_cli_session`, `spawn_split_agent`:
    9 → 11 parameters).
  - **Recipe B (recommended):** methods on a per-topic **owned handle** whose fields have the host's
    names, for example `impl AgentRoster { … }`. Moving a method from `impl DaemonSessionHost` to
    `impl AgentRoster` changes only the `impl` header. `self.config` stays `self.config`, and
    `let service = self.clone(); tokio::spawn(…)` stays **textually identical**, because the handle
    is `Clone`. Parameter counts are unchanged, so the conversion diff is mostly `impl` lines and
    callback call sites. node 17 moves the type with its methods (no `E0116`).

  Recipe B's cost is building a handle per call: a `DaemonConfig` clone, which a host `self.clone()`
  already does today. Two ways to avoid even that:
  - (i) the host keeps the handles as fields. But the `with_*`/`set_*` builders mutate
    `room_roster`, `session_rooms`, `staging_base_dir`, …, so each builder must update the handle
    too, a behaviour risk;
  - (ii) the host's `config` becomes `Arc<DaemonConfig>`, a field-type change across the host.

  **Recommended: B, built per call.** The clone cost matches today's hand-offs.
- **D2: `AgentHostCallbacks`.** Add **one** method, `session_room_roster` (for `ensure_session_room`).
  The other six of the seven are state reads or free functions (see "State structs and callback
  traits"). That needs `AgentRosterState` widened by `session_admissions` and `model_registry`. Is
  that preferable to widening the trait? Recommended: yes. The seventh, `ensure_session_room`,
  becomes T3's own function.
- **D3: `SplitHost` and `LaunchHost` beyond the plan.**
  - `SplitHost` gains `session_files` and `session_agents` (both wiring services). It inherits
    `worktree_snapshot` through `SplitHost: AgentHostCallbacks`, and drops the plan's
    `agent_tool_socket` (a free fn).
  - `attached_initial_prompt` **moves into T4** rather than becoming a callback (recommended).
  - `DaemonSeedCloneClaimant` holds the agents handle and moves with T3, so `LaunchHost` stays at the
    plan's `{sandbox_rpc_handler, pr_stack}`. The alternative is `LaunchHost::seed_clone_claimant`.
  - `impl StackParentHost` moves from the host onto the launch handle.
- **D4: a new crate `tddy-cli-sessions`** for the PTY runtime (`cli_session_manager` and its 9
  children, `session_toolcall`; 1,709 lines).
  - Why it is needed: split names `CliSessionManager` and `PtyHandle`, and split sits below launch.
  - The alternative: fold the runtime into `tddy-session-split` (~4.9k). It is simpler, but a
    split-session crate owning the daemon's PTY runtime is a misleading home.
  - `tddy-terminal-rpc` is not an option: it would drag the sandbox stack and activity into
    `tddy-coder`.
- **D5: `service_util` and `workspace_session` → `tddy-session-split`.**
  - Why: both are used by T1 above and T4, and `split_start` names `workspace_session::PairedAgentSession`.
  - `resolve_worktree_root_for_session` goes to T3, because agents calls it.
  - The alternative home for `service_util` is `tddy-worktree-service`, which would gain
    `tddy-semantic-index` and `chrono`.
- **D6: no `AdmissionState` struct.** The admission-token function takes `config` and
  `session_admissions` directly. This also avoids the `OsUserResolver` name collision with
  activity's alias.
- **D7: `LocalExecTools`.**
  - **A (the default under the approved callbacks):** it stays in lifecycle (+231 wiring).
  - **B:** it moves to `tddy-session-agents`, taking `ExecToolRoute` with it. Agents gains
    `tddy-daemon-sandbox`, `tddy-sandbox-runner`, `tddy-tool-engine` and `tddy-task`, and the
    `local_exec_tools` and `run_exec_tool_locally` callbacks become unnecessary.
- **D8: re-parent four files by hand** (`git mv` plus `mod` edits; the engine cannot). A module move
  takes its children, so node 17 needs these apart:

  | Parent (topic) | Child (topic) |
  |---|---|
  | `svc_resolve_tddy_tools_path.rs` (T4) | `svc_host_builders` and its three children (W / T7 / T10) |
  | `svc_materialize_staged_attachment.rs` (T8) | `split_claude_cli_start` (T4) |
  | `svc_resolve_os_user.rs` (W) | `session_attachment_materialization` (T8) |
  | `svc_turn_end_reporter.rs` (T3) | `jail_env_builders` (T1) |

  The alternative is node 17 moving each nested module individually, where the engine allows it,
  and leaving the parents' declarations. That is untested for a parent that itself moves.
- **D9: relocate `svc_split_context_from_codebase_host.rs`'s inline test.** It constructs a
  `DaemonSessionHost`, so it must stay in lifecycle when T4 moves. It becomes a
  `connection_service/split_context_from_codebase_host_tests.rs` sibling. This is a move of test
  code within the crate.
- **D10: converting the unexecuted sandboxed paths.**
  - `crap-svc-start-sandboxed-cursor-cli-session.md` says "tests first", and
    `start_sandboxed_claude_cli_session` is never executed on macOS either.
  - The options:
    - (a) convert them under Recipe B's header-only edit, with Linux CI as the evidence;
    - (b) write characterisation tests first, which is the jail-launch todo's work and a node of its
      own;
    - (c) leave them host-bound, which blocks T1's move and the target.
  - Recommended: (a).
- **D11: shape tests.** A1–A6 are greps by default, honouring the 2026-09-25 "no shape tests". A
  shape test would make them regressions CI catches.
- **D12: T11's receiver.** `tddy-demo-runner` (156 lines today) needs `tddy-daemon-kernel`,
  `tddy-core`, `tddy-rpc` and `tddy-service`. `tddy-vm` (7.2k, a low-level crate that `tddy-vm-build`
  and `-testkit` consume) would push daemon-kernel into their graphs. Recommended: demo-runner.
- **D13: the size target.**
  - The code says **~4.4k–4.6k** with `PeerRouted*` staying. ~3.4k needs `PeerRouted*` behind a
    forwarding port (→ ~3.6k–3.8k) plus the adapters as receiver-side impls (→ ~3.3k–3.5k).
  - The choice: accept ~4.5k as node 17's target, or approve those two moves (as a later node).
- **D14: lifecycle's unused dependencies.**
  - Seven normal dependencies are unused: `tddy-coder`, `tddy-bsp`, `tddy-demo-runner`, `tddy-lsp`,
    `tddy-lsp-executor`, `tddy-screen-sharing` and `tddy-actions`.
  - Two are test-only: `tddy-connectrpc` and `tddy-session-sync`.
  - The choice: drop them in node 17, file a todo, or leave them.
  - After node 17, lifecycle may also no longer need `tddy-vm`, `tddy-semantic-index`,
    `tddy-stdio` and others.
- **D15: ~~cut this node into five linear siblings (16a–16e), or keep one PR~~** ✅ **Decided
  2026-09-26: cut.** See "Settled by the developer" and "Milestones and the PR cut".

**Which node needs which decision.** Each successor changeset copies the D-items it needs, still open,
with the recommendation.

| Decision | Needed before | Carried in |
|---|---|---|
| D1 recipe shape and hand-offs | 16a (it fixes every state's shape) | 16a, 16b, 16c, 16d, 16e |
| D2 `AgentHostCallbacks` + `session_room_roster` | 16b | 16b |
| D3 `SplitHost` / `LaunchHost` beyond the plan, `DaemonSeedCloneClaimant`, `StackParentHost` | 16b (claimant), 16c (`SplitHost`), 16d (`LaunchHost`, `StackParentHost`) | 16b, 16c, 16d, 16e |
| D4 `tddy-cli-sessions` | 17 (a crate); 16c only treats CLI as its own topic | 16c (informational), 17 |
| D5 `service_util`, `workspace_session` → split | 16a (its T3 half, M0.1), 16c (placement) | 16a, 16c, 17 |
| D6 no `AdmissionState` | 16a | 16a |
| D7 `LocalExecTools` | 16b (callback set), 17 (move) | 16b, 17 |
| D8 re-parent four files | 16a | 16a |
| D9 relocate the inline test | 16a | 16a |
| D10 converting the unexecuted sandboxed paths | 16d | 16d |
| D11 shape tests | every node | 16a–16e, 17 |
| D12 T11's receiver | 17 (16a's `DemoVmState` is the same either way) | 17 |
| D13 the size target | 17 | 17 |
| D14 lifecycle's unused dependencies | 17 | 17 |
| Edge approvals ("New crate edges") | 17 | 17 |

### Carried from the plan, corrected by the code

- `ensure_project_available_for_start` is **T1, not T3**. It moves to agent-launch, so no session-agents
  → `tddy-spawn` edge is needed.
- `remote_git_pack_execution` is a host-free leaf that no plan placed. It goes to
  `tddy-session-activity`, with zero edges.
- `split_session` (698 lines) is entirely host-free. It waits only for `tddy-session-split` to exist.
- `DemoVmState` needs `config`. `AttachmentState` and `PresenterObserverDeps` are exactly as planned.
- The plan's `SplitHost::agent_tool_socket` is not needed.
- The four planned cycle cuts are not enough: eight more are listed in the matrix.

## Validation results

(Empty. Filled per milestone during `/green`.)

## TODO

- [x] Inventory, port design, budget, graphs, from the code at `9d464a8e`
- [x] Create changeset: this document
- [x] D15 decided: cut into 16a–16e + node 17; successor changesets written
- [ ] USER REVIEW: D1, D5 (T3 half), D6, D8, D9, D11 before 16a's `/green`; the rest before their node
- [ ] Rebase onto #526 once it is green
- [ ] Record the baseline (M0)
- [ ] Implementation M0–M3
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`): this document is wrapped first, so the successor
  changesets carry their own copies of what they need

## Final Checklist

Tasks executed at wrap:

**16a acceptance (the "Acceptance criteria for 16a")**
- [ ] A1: no 16a topic file names `DaemonSessionHost` (grep empty)
- [ ] A2: no 16a topic file names a wiring module (grep empty)
- [ ] A3: T7, T8, T10, T11 and the leaves name no other topic, and the six M0 cuts hold (scripted grep plus six named greps; `cargo modules` if available)
- [ ] A3b: the four children re-parented (D8) and the D9 test relocated (greps empty)
- [ ] A4: 16a topic files name foundations and receivers by their defining crate (grep empty)
- [ ] A5: no host clone in a 16a topic file (grep empty)
- [ ] A6: no callback trait and no `AdmissionState` exist yet (greps empty)
- [ ] A7: no consumer edit and `tddy-session-agents` untouched (`git diff` empty); `cargo check --all-targets` clean on lifecycle, `tddy-session-agents`, `tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`
- [ ] A8: baseline 562 / 22 / 1, the same 22 by name, after M0, M1, M2 and M3; `restructure verify` accounted

**Documentation**
- [ ] `packages/tddy-session-lifecycle/docs/module-layout.md`: the 16a topic modules, the three new modules and the four re-parented files (via the changeset workflow)
- [ ] `packages/tddy-session-lifecycle/docs/changesets/2026-09-26-carve-lifecycle-ports.md`: release-note entry with the before and after numbers
- [ ] The M0/M3 code issues touched here re-measured; none is closed by 16a
- [ ] Master plan content the successors need is present in their own changesets (forward-only rule: none links back here)

## Successor PRs

Forward links only (parent → child). Each successor changeset is self-contained and does not link back.

1. **16b** `#carve 17/21`, `feature/carve/lifecycle-ports-agents`: [2026-09-26-carve-lifecycle-ports-agents.md](./2026-09-26-carve-lifecycle-ports-agents.md), M4 T3 agents
2. **16c** `#carve 18/21`, `feature/carve/lifecycle-ports-split`: [2026-09-26-carve-lifecycle-ports-split.md](./2026-09-26-carve-lifecycle-ports-split.md), M5 T4 + `service_util` + `workspace_session`
3. **16d** `#carve 19/21`, `feature/carve/lifecycle-ports-launch-spawns`: [2026-09-26-carve-lifecycle-ports-launch-spawns.md](./2026-09-26-carve-lifecycle-ports-launch-spawns.md), M6 T9 + M7a
4. **16e** `#carve 20/21`, `feature/carve/lifecycle-ports-launch-start`: [2026-09-26-carve-lifecycle-ports-launch-start.md](./2026-09-26-carve-lifecycle-ports-launch-start.md), M7b + M8 T1c
5. **17** `#carve 21/21`, `feature/carve/lifecycle-moves`: [2026-09-26-carve-lifecycle-moves.md](./2026-09-26-carve-lifecycle-moves.md), every converted topic moved into its receiver
