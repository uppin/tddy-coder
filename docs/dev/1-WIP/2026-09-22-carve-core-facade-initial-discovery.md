# Initial discovery — carve-core-facade (`#carve` 12)

**Changeset:** [2026-09-22-carve-core-facade.md](./2026-09-22-carve-core-facade.md)
**Trees explored:**
- `feature/carve/rpc-handlers` @ `9f08a0e6` (the stack tip) for the module graph;
- `origin/master` @ `65a7c307` for sizes, because #493 merged there after the stack was cut.

## Combined conclusions

1. **Every group in tddy-core can leave, and no receiving crate comes near 10k.** The largest is the
   agent-backend crate at 5,814 production lines. As a facade, `tddy-core` keeps about 140 lines:
   `lib.rs`, the three residue facades #493 left, `ssh_exec` and the `cfg(test)` `test_support`.
2. **tddy-core is already smaller than planned.** #493 (`#carve` 7) **merged to master** on 2026-09-22
   (`65a7c307`). It moved `session_actions` (except `session_dir.rs`), `atomic_file`, `output`,
   `error` and the SQLite catalog into `tddy-session-store` (1,625) and `tddy-session-catalog` (747).
   On master tddy-core is **20,924** production lines, not the 23,418 this stack's branches show.
   **The `#carve` stack was cut before #493 merged and is behind master.** It must be cascaded onto
   master before this node can go green.
3. **1,090 production lines are dead and never compiled.** `workflow/mod.rs:43-61` declares `context`,
   `graph`, `hooks`, `runner`, `session` and `task` as inline modules re-exporting `tddy_graph::*`,
   so `workflow/{context 120, graph 145, hooks 55, runner 163, session 112, task 495}.rs` are never
   loaded. They also create false edges in any grep census (for example `task.rs:5` → backend).
4. **The one real cycle is small.** The strongly connected set is
   `backend → toolcall → session_actions → changeset → workflow → backend`. It breaks with:
   - **Cut 1:** move `PermissionHint` and `GoalHints` (`workflow/recipe.rs:18-38`, plain data) into
     `tddy-workflow`. These are the only real backend → workflow edges: `backend/claude.rs:6`,
     `backend/codex.rs:17`, `backend/mod.rs:14`, `:459`.
   - **Cut 2:** move `start_goal_for_session_continue` (`changeset/merge.rs:114-168`) up to the
     workflow engine. It is the only changeset item that names `WorkflowRecipe` (`merge.rs:14`).
     Callers: `presenter/workflow_runner.rs:847` and `tddy-workflow-recipes/src/pr_stack/mod.rs:679`.
   - **Two retargets:** `error.rs:3` and `toolcall/mod.rs:127` name `ClarificationQuestion` through
     `crate::backend`, but it already lives in `tddy-workflow`.
5. **`docs/dev/todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md`
   rests on a false premise.** It says moving `GoalHints`/`PermissionHint` "buys nothing" because
   `WorkflowRecipe` and `CodingBackend` name each other. **No compiled backend file names
   `WorkflowRecipe`**: the only mention is a doc comment at `backend/claude.rs:327`, and the only
   `crate::backend::WorkflowRecipe` import is in the dead `workflow/task.rs:5`. `workflow/recipe.rs:3`
   names `CodingBackend`, so the edge runs one way, workflow → backend. Cut 1 resolves the entry.
6. **Why #493 left `session_dir.rs`, `session_action_jobs` and `session_action_pipeline` behind.**
   They read the changeset (`read_changeset`), which sat in tddy-core's cycle. Once the changeset has
   its own crate, they can move to a crate above it. They cannot go into `tddy-session-store`: the
   changeset needs `atomic_file` and `error` from there, so that would close a cycle.
7. **Every public path must survive as a facade.** tddy-core has 41 `pub mod` paths, about 35
   root-level backend re-exports and grouped `pub use` blocks. Consumers name
   `tddy_core::<module>::…` heavily: `workflow` from 330 sites in `tddy-workflow-recipes` alone,
   `changeset` from 112 files. There is also a `workflow_decouple_acceptance` guard in `lib.rs` that
   forbids re-exporting `plan_prd_path_for_session_dir`.
8. **No receiving crate can be one that depends on tddy-core.** `tddy-session-activity`,
   `tddy-session-agents` and `tddy-tui` all do. `tddy-workflow` (deps `log` and `serde` only),
   `tddy-git`, `tddy-graph`, `tddy-task` and `tddy-stdio` do not.
9. **Heavy dependencies leave with their modules.**
   - `agent-client-protocol` and `tokio-util` go with backend.
   - `jsonschema` goes with `session_action_pipeline`.
   - `futures` is used nowhere and can simply be dropped.
10. **Crate-root imports break on any move.** Many files name `crate::X` rather than
    `crate::module::X` (the list is in Exploration 1 §A). Each needs retargeting as its module moves.

## Exploration 1 — module graph, consumers and receivers (Explore agent, very thorough)

Scope: the nine candidate groups, their intra-core edges, external consumers, candidate receiving
crates, dependency attribution, root re-exports, code-issue claims, and extraction order.

### Proposed partition (from the stack tree, before #493)

| # | Crate | Modules | Prod lines | Depends on | Blocking cut |
|---|---|---|---:|---|---|
| 0 | *(delete)* | the six dead `workflow/*.rs` files | −1,090 | — | none |
| 1 | `tddy-workflow` (existing) | + `GoalHints`, `PermissionHint` | +~25 | none | Cut 1 |
| 2 | `tddy-log` | `log_backend` 862, `stdio_safety` 65 | 927 | none | none |
| 3 | store group | `atomic_file`, `error`, `output`, `session_lifecycle`, changeset (minus Cut 2), `branch_worktree_intent`, the session-metadata modules, `agent_activity`, `elapsed_format`, `source_path` | 2,828 | workflow, graph | Cut 2 + retarget `error.rs:3` |
| 4 | `tddy-session-worktree` | `worktree` 428, `base_sync` 302, `session_chain` 363, `git_head` 165 | 1,258 | store, git | none |
| 5 | session actions | `session_actions`, `session_action_jobs`, `session_action_pipeline` | 2,304 | store, actions, task | none |
| 6 | `tddy-session-catalog` | `session_catalog` | 747 | session actions, task | none |
| 7 | `tddy-toolcall` | `toolcall` | 1,451 | session actions, rpc, stdio, workflow | retarget `toolcall/mod.rs:127` |
| 8 | `tddy-agent-backend` | `backend` 4,684, `stream` 884, `token_accounting` 76, `claude_argv` 42, `claude_hooks` 69, `cursor_hooks` 41, `spawn_env` 18 | 5,814 | toolcall, store, workflow | Cut 1 |
| 9 | `tddy-agent-skills` | `agent_skills` 410, `feature_start_slash` 117 | 527 | none | none |
| 10 | `tddy-workflow-engine` | workflow's compiled files 1,793, + Cut 2's function (55) | 1,848 | agent-backend, toolcall, store, graph | Cut 2 lands here |
| 11 | `tddy-presenter` | `presenter`, `post_workflow` 210, `usage_watcher` 158 | 4,486 (4,330 on master) | engine, backend, toolcall, store, log, skills, session-worktree | `usage_watcher` must go with presenter |

### Intra-core edges (production only)

- **backend:**
  - uses `error::BackendError`, `atomic_file::write_atomic`, `toolcall::SubmitResultChannel` and
    `toolcall::transition_handler`, and workflow via Cut 1;
  - `usage_watcher.rs:29` names `crate::PresenterEvent`, so it belongs with presenter.
  - Inbound from workflow: `agent_output.rs:6`, `backend_invoke_task.rs:6`/`:258`,
    `engine.rs:5`/`:13`, `mod.rs:6`/`:80`, `recipe.rs:3`. Inbound from presenter: `SharedBackend`,
    `CodingBackend`, `QuestionOption`, `token_accounting::ConversationRecord`.
- **presenter:** uses backend; workflow (`WorkflowController`, the sinks, `WorkflowEngine`); changeset
  (`workflow_run.rs:166-356`, `workflow_runner.rs:14`, `:451-855`); toolcall
  (`agent_session_runner.rs:87-118`); `session_metadata::write_initial_tool_session_metadata`;
  `log_backend`; `git_head::read_head_commit`; `post_workflow`.
  - Inbound: `session_lifecycle.rs:6` and `session_dir.rs:75` use `output`, and `agent_skills.rs:302`
    uses `feature_start_slash`.
- **session actions:** `session_dir.rs:17` uses `crate::{read_changeset, WorkflowError}`; `runtime.rs`
  and `session_action_jobs/runner.rs` use `atomic_file`. Inbound: `toolcall/listener.rs:10`.
- **workflow:**
  - out: `toolcall::take_submit_result_for_goal`, `TransitionHandler`; changeset `read_changeset`,
    `update_state`, `write_changeset_atomic`, `Changeset`;
    `session_lifecycle::resolve_effective_session_id`.
  - changeset out: `session_lifecycle::unified_session_dir_path` (`stack.rs:288`/`302`/`330`),
    `branch_worktree_intent` (`merge.rs:65`), `WorkflowRecipe` (`merge.rs:14`, Cut 2).
  - `worktree` → changeset and `branch_worktree_intent`; `session_chain` → changeset, `session_lifecycle`,
    `worktree`; `base_sync` → `worktree`.
- **toolcall:** out `crate::ClarificationQuestion` (retarget) and session actions.
- **session metadata:** out `output`, `read_changeset`, `session_agent`, `error`, `atomic_file`.
- **log:** a leaf. Its only core consumer outside presenter is `stdio_safety.rs:4`.

**Crate-root imports that break on a move:** `session_dir.rs:17`, `session_action_jobs/runner.rs:14`,
`session_metadata.rs:184-288`, `session_chain.rs:12`, `presenter_impl.rs:7`, `backend_selection.rs:6`,
`questions.rs:10`, `wiring.rs:15`, `workflow_run.rs:15`, `workflow_runner.rs:14`, `activity.rs`,
`agent_activity_stamping.rs`, `engine.rs:13`, `backend_invoke_task.rs:357-368`, `usage_watcher.rs:29`,
`toolcall/mod.rs:127`.

### External consumers (`tddy_core::<module>` references per crate)

- **backend:** integration-tests 120, workflow-recipes 53, coder 42, tui 15, service 15,
  discovery 14, session-lifecycle 7, e2e 6, tools 5, telegram-control 5, others ≤4.
- **presenter:** tui 65, service 49, coder 35, e2e 14, workflow-recipes 13.
- **agent_activity:** session-lifecycle 18, daemon-sandbox 16, daemon-livekit 11,
  session-activity 11, service 9.
- **workflow:** workflow-recipes 330, integration-tests 168, coder 29, session-lifecycle 25,
  pr-stack 7.
- **changeset:** workflow-recipes 166, session-lifecycle 89, pr-stack 75, integration-tests 70,
  coder 32, telegram-control 21, tools 21.
- **toolcall:** session-lifecycle 16, tools 14, coder 13, tool-engine 7, lsp-executor 5.
- **session_metadata:** session-lifecycle 157, telegram-control 19, daemon 13, coder 11.
- **log_backend:** integration-tests 17, coder 10, spawn 5.
- **agent_skills:** tui 15, coder 10.

### Receiving-crate eligibility

- `tddy-workflow`: deps `log` and `serde` only; already owns `GoalId`, `WorkflowState`,
  `ClarificationQuestion`, `QuestionOption`, `ProgressEvent` and `WorkflowEvent`.
- `tddy-session-activity`, `tddy-session-agents`, `tddy-tui`: all depend on tddy-core, so they
  **cannot** receive code from it.
- `tddy-task`, `tddy-stdio`, `tddy-git`, `tddy-graph` and `tddy-actions` do not depend on tddy-core.

### Dependency attribution

| Dependency | Used by |
|---|---|
| sqlx | `session_catalog` (moved by #493) |
| agent-client-protocol, tokio-util | `backend/{acp,codex_acp,mod}` |
| jsonschema | `session_action_pipeline` |
| tddy-rpc, tddy-stdio | `toolcall/{client,listener}` |
| tddy-git | `worktree`, `ssh_exec` |
| tddy-graph | `workflow/{mod,controller,backend_invoke_task}` |
| futures | **unused** |

### Code issues (`packages/tddy-core/docs/code-issues/`, 22 files)

- `heavy-dependency-sqlx-session-catalog.md`: claimed by **#493**, now **merged**. The claim is stale.
- `squatting-git-plumbing-worktree.md`: claimed by **#492**, **merged**. Stale: `tddy-git` exists and
  `worktree.rs` is down to 428 lines.
- `cycle-dto-inside-behaviour-module.md`: claimed by "nobody"; points at the backend TODO in
  conclusion 5.
- 19 `complexity-*` records, all unclaimed. Two presenter ones are marked partly fixed by #495.

### Suggested extraction order (leaves first)

1. Delete the dead files; drop `futures`.
2. Cut 1 and the two retargets.
3. Extract `tddy-log` and `tddy-agent-skills`.
4. Cut 2.
5. Extract the changeset and metadata crate, then `tddy-session-worktree`, then session actions.
6. Extract `tddy-toolcall`, then `tddy-agent-backend`, then `tddy-workflow-engine`, then
   `tddy-presenter`.
7. `tddy-core` becomes the facade.

Also noted: the 49 files in `packages/tddy-core/tests` split with their code.

## Exploration 2 — master after #493 (by hand)

Production lines per module on `origin/master` @ `65a7c307`:

| Crate | Production | Notable |
|---|---:|---|
| tddy-core | **20,924** | `backend` 4,684, `presenter` 3,962, `workflow` 2,883 (1,090 dead), `toolcall` 1,451, `changeset` 1,018, `stream` 884, `log_backend` 862, `session_action_jobs` 539, `session_action_pipeline` 435, `session_actions` residue 119, `output` / `error` / `atomic_file` 4 each (facades) |
| tddy-session-store | 1,625 | `session_actions` 1,225, `atomic_file` 181, `output` 148, `error` 62 |
| tddy-session-catalog | 747 | |

#493's wrap (`docs/dev/changesets/2026-09-22-carve-session-store.md` on master) records why the
three residues stayed:
- `session_dir.rs` reads the changeset (`session_actions/session_dir.rs → changeset`, since #474).
- `session_action_jobs::runner` needs `read_changeset`, and reaches the runtime through seven
  `#[doc(hidden)] pub` functions.
- `jsonschema` stays in tddy-core only because `session_action_pipeline.rs` still names it.

Checked by hand:
- `grep -rn WorkflowRecipe packages/tddy-core/src/backend/` returns **one doc comment**
  (`claude.rs:327`).
- `workflow/recipe.rs:3`: `use crate::backend::{ClarificationQuestion, CodingBackend}`.
- `workflow/mod.rs:43-61` holds the inline shim modules (conclusion 3).
