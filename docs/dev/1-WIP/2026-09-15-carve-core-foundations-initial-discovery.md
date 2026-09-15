# Initial discovery — `#carve` 4/9 `core-foundations`

**Scope:** `tddy-core`, `tddy-session-lifecycle`, `tddy-workflow-recipes`.
**Explorations 1–3 are the whole-work dump for the `#carve` stack, copied here in full.** Exploration 4 is this node's own.

## Combined Conclusions

*(rewritten after each exploration pass; see the passes at the tail for evidence)*

### Measurement method

"prod" lines = everything before the **first** `#[cfg(test)]` in a file. Verified sound for files
with a single trailing test module (`presenter_impl.rs`, `worktree.rs`, `changeset.rs`,
`pr_stack/mod.rs`, `parser.rs`, `telegram_session_control.rs`, `telegram_notifier.rs`). It is
**wrong** for `tddy-session-lifecycle/src/connection_service.rs`, which interleaves
`#[cfg(test)] use` declarations from line 44 — that file is ~1,061 substantive lines, not 41.

### Sizes that drive the decomposition

| Package | Total | src prod | src inline tests | `tests/` | Test bins |
|---|---:|---:|---:|---:|---:|
| `tddy-core` | 40,419 | ~24,200 | 8,785 | 7,400 | 44 |
| `tddy-session-lifecycle` | 38,083 | ~31,700 | 6,337 | 0 | 0 |
| `tddy-workflow-recipes` | 33,785 | ~16,000 | 7,070 | 10,388 | 56 |

### `tddy-core` — seven concerns, 35 dependents

| Concern | Lines | Intra-crate deps |
|---|---:|---|
| `backend/` — Claude/Codex/Cursor/ACP adapters | 5,753 | `stream`, `toolcall`, `workflow`, `atomic_file`, `error`, `token_accounting` |
| `presenter/` | 5,213 | nearly everything |
| `workflow/` | 3,867 | `backend`, `changeset`, `presenter`, `toolcall`, `session_lifecycle` |
| `worktree.rs` | 2,375 | `changeset`, `branch_worktree_intent`, `ssh_exec` |
| `toolcall/` | 2,132 | `backend`, `presenter`, `session_actions`, `workflow` |
| `changeset.rs` | 1,627 | `backend`, `workflow`, `worktree`, `atomic_file`, `error`, `session_lifecycle`, `branch_worktree_intent` |
| `session_catalog/` | 854 | `session_actions`, `toolcall` |
| `log_backend.rs` | 1,055 | **none** |

Cycles present: `changeset ↔ worktree`, and `backend ↔ workflow ↔ presenter ↔ toolcall`.

**Heavy-dependency isolation (the compile-time argument):**
- `sqlx` (bundled SQLite) is named by **only 4 files**, all under `session_catalog/`. All **35
  crates that depend on `tddy-core` currently compile it.**
- `agent-client-protocol` is named by only `backend/{mod,acp,codex_acp}.rs`.
- `jsonschema` is named by only `session_action_pipeline.rs` and `session_actions/validate.rs`.

**`Presenter` is a god object**, not merely a big file: one struct, **37 fields**, **46 methods**,
1,788 production lines in `presenter/presenter_impl.rs`. The field clusters are already named by
its own doc comments: workflow-run state, pending-question/elicitation state, agent-activity
recording, broadcast/view channels, backend+recipe selection.

### `tddy-session-lifecycle` — one back-edge gates a 7,400-line cut

The Telegram cluster is **7,403 lines across 6 files** (19% of the crate) and is the crate's only
`teloxide` user:

| File | Total | Prod |
|---|---:|---:|
| `telegram_session_control.rs` | 4,476 | 3,980 |
| `telegram_notifier.rs` | 1,745 | 1,239 |
| `telegram_bot.rs` | 788 | 788 |
| `telegram_session_subscriber.rs` + `telegram_multi_select_shortcuts.rs` | 231 | 231 |
| `session_notification_subscribers.rs` | 163 | 163 |

**Its outbound edges are all in the acyclic direction** (telegram → `cli_session_manager`,
`session_deletion`, `session_list_enrichment`, `cursor_cli_spawn`, `session_reader`,
`project_storage`, `branch_owner`, `presenter_intent_client`, `active_elicitation`, `elicitation`,
`config`, `user_sessions_path`).

**The single blocking back-edge is one field and one call site:**

- `connection_service.rs:141` — `telegram: Option<Arc<TelegramDaemonHooks>>`
- `connection_service/svc_resolve_tddy_tools_path.rs:439` — the only consumer, passing it to
  `telegram_session_subscriber::spawn_presenter_observer_task(...)`

`telegram_session_subscriber.rs` (129 lines) is the whole bridge. Replacing the field with an
injected port removes the last cycle.

`telegram_session_control.rs` internal shape: ~1,200 lines of DTOs + ~30 pure `parse_*` functions,
then **a single 2,634-line `impl TelegramSessionControlHarness` block** (lines 1272–3906) holding
57 methods in six visible clusters (pickers, elicitation, session start, chaining, recipe/plan
review, list/delete/enter).

`connection_service/` is 18,771 lines over 65 files — **already decomposed** by the earlier
`#unbundle` work; it is not a primary target. It does carry restructure residue: stacked duplicate
`#[cfg(test)]` attributes at `connection_service.rs:44–48`.

### `tddy-workflow-recipes` — two non-recipe subsystems squatting

Roughly **6,750 of ~16,000 production lines are not recipes**:

| Squatter | Lines | Belongs to |
|---|---:|---|
| GitHub REST client — `orchestrate_pr_stack/github.rs` (1,292) + `github_pr.rs` (494) + `github_rest_common.rs` (338) | **2,124** | `tddy-github`, which already exists (auth/tokens only, 1,340 lines) and which this crate **does not depend on** |
| PR-stack data model — `pr_stack/mod.rs` minus its recipe impl (~1,550) + `pr_stack/docs.rs` (447) + `orchestrate_pr_stack/{assess,git_ops,pr_insight,actions}` (~2,290) | **~4,280** | a new `tddy-pr-stack` |

`pr_stack` + `orchestrate_pr_stack` are the **most-referenced external surfaces** of the crate
(79 of ~190 cross-crate references), so 12 dependents pull in every recipe to get a stack data model.

`pr_stack/mod.rs` has a clean internal line: `PrStackRecipe` / `impl WorkflowRecipe` /
`impl SessionArtifactManifest` occupy lines 131–418; lines 418–1958 are pure stack operations with
no recipe coupling.

`parser.rs` (1,216 prod) is **six independent phase parsers** sharing only `ParseError`.

### Out of scope, recorded for the record

`tddy-daemon` (58,693 lines) was measured in the same pass and is **not** in this stack's scope.
Its shape is relevant background: `src/` is only 2,377 production lines, `lib.rs` is a 30-line
facade re-exporting ~90 modules from `tddy-session-lifecycle`, and its 139 test binaries
(55,727 lines) are `tddy-session-lifecycle`'s acceptance suite parked in the wrong crate. **16 of
its `tddy-*` runtime dependencies are named by no file in its `src/`.** Any node here that moves a
module out of `tddy-session-lifecycle` must update that facade, and the daemon's telegram test
suites (12 files, 4,901 lines) reach the cluster through it.

---

## Exploration 1 — sizing the workspace and the four largest crates

Run by the parent (Bash: `find`, `wc`, `grep`, `awk`, `sed`) in the session preceding
`/plan-pr-stack`, to answer "list packages by LoC" and then "what are the biggest parts and how
could they be split".

### Sequence

1. `ls packages/` — enumerate the workspace (84 packages).
2. `find ... -name '*.rs' -o -name '*.ts' ... | xargs cat | wc -l` per package, excluding
   `target/`, `node_modules/`, `dist/`, `storybook-static/` — rank packages by LoC.
3. `find <pkg> -type d` on the four largest — directory shape.
4. `find ... \( -name '*.rs' -o -name '*.proto' \) | xargs wc -l | sort -rn` — largest files per
   package.
5. Per-package `src` vs `tests` vs `generated` vs `proto` line and file counts.
6. `awk 'FNR==1{started=0} /#\[cfg\(test\)\]/{started=1} started{c++}'` — inline test-line estimate.
7. `grep -n '^pub fn|^impl|^pub struct|^pub enum'` on each oversized file — item outlines.
8. `grep -rhoE 'crate::[a-z_]+'` per `tddy-core` module — intra-crate dependency map.
9. `grep -l 'tddy-core = ' */Cargo.toml | wc -l` — reverse-dependency counts.
10. `grep -rl '<dep>' tddy-core/src` for `sqlx`, `jsonschema`, `agent_client_protocol` — heavy-dep
    isolation.
11. `for d in $(grep -oE '^tddy-[a-z-]+ = ' Cargo.toml); do grep -rq "$d" src/; done` on
    `tddy-daemon` — which declared deps its `src/` actually names.
12. `grep -rhoE 'tddy_workflow_recipes::[a-z_]+' --include='*.rs' .` — external surface usage.
13. `ls docs/dev/todo/`, `grep -ril 'module split|too large|LoC' docs/dev/todo/` — prior art.

### Inspected files

- `packages/tddy-core/Cargo.toml` — `sqlx` comment ("First DB dependency in the workspace
  (per-session `session_catalog` SQLite store)"), the `anyhow` exception recorded at `#unbundle`
  node 5, and the `tddy-sandbox` dev-dependency rationale.
- `packages/tddy-session-lifecycle/Cargo.toml` — ~60 path dependencies, each with a comment
  recording which `#unbundle` node moved it.
- `packages/tddy-daemon/Cargo.toml` — near-identical dependency list to the above.
- `packages/tddy-daemon/src/lib.rs` — the two `pub use tddy_session_lifecycle::{...}` blocks
  re-exporting ~90 module paths under the comment *"Legacy paths for integration suites
  (`tddy_daemon::connection_service`, …)"*.
- `packages/tddy-daemon/src/{config,tddy_user_config,user_sessions_path,relay_idle}.rs` — four
  2-line re-export shims.
- `packages/tddy-core/src/presenter/presenter_impl.rs:55-136` — the 44-field `Presenter` struct,
  read in full for the field clustering.
- `packages/tddy-session-lifecycle/src/session_notification_subscribers.rs` — read in full;
  it *is* Telegram (holds `Arc<TelegramDaemonHooks>`, reads the `telegram:` config block).
- `packages/tddy-workflow-recipes/src/github_pr.rs:1-25` and
  `orchestrate_pr_stack/github.rs:1-25` — both headed "GitHub REST"; no `tddy-github` dependency.
- `docs/dev/changesets/2026-09-13-unbundle-post-stack-follow-ups.md` — node 10 of 10; its
  backlog list names "module splits" as deliberately deferred.
- `docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md` — the file-budget
  precedent, and the reason two `service.rs` files were left whole ("that reason expires when the
  stack lands").
- `docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md` — D6–D9;
  the import-restoration defects that blocked cross-crate `extract_module` are **fixed**.
- `docs/dev/todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md` — CRAP scores;
  `telegram_bot.rs` holds the crate's two worst functions (complexity 88 and 52, both untested).

### Grep / glob — notable hits

- `grep -rl sqlx tddy-core/src` → exactly `session_catalog/{populate,error,store,read}.rs`.
- `grep -rl agent_client_protocol tddy-core/src` → `backend/{codex_acp,mod,acp}.rs`.
- `grep -l 'tddy-core = ' packages/*/Cargo.toml | wc -l` → **35**.
- `grep -l 'tddy-workflow-recipes = ' packages/*/Cargo.toml | wc -l` → **12**.
- `grep -l 'tddy-session-lifecycle = ' packages/*/Cargo.toml | wc -l` → **1** (`tddy-daemon`).
- `grep -rhoE 'tddy_workflow_recipes::[a-z_]+'` → `pr_stack` 41, `orchestrate_pr_stack` 38,
  `tdd` 23, `bugfix` 16, `github_pr` 15, `schema` 13.
- `grep -rl 'tddy_session_lifecycle' tddy-daemon/tests/` → **0 of 139**;
  `grep -rl 'tddy_daemon::' tddy-daemon/tests/` → **133**.
- `grep -n 'tddy-github' tddy-workflow-recipes/Cargo.toml` → no match.
- `ls packages/*/tests/*.rs | wc -l` → 597 workspace test binaries; 139 of them in `tddy-daemon`.

### Findings

Established the size table, the per-crate concern inventory, the `tddy-core` intra-crate
dependency map (including the two cycles), the heavy-dependency isolation that makes
`session_catalog` and `backend/` worth extracting, the `Presenter` god-object shape, the
`tddy-workflow-recipes` GitHub/PR-stack squatters, and the `tddy-daemon` facade that makes that
crate's size an artifact of misplaced tests rather than of production code.

---

## Exploration 2 — precise intra-crate edges (comment-free, production-only)

Run by the parent during `/plan-pr-stack` Step 1/2, because Exploration 1's module map was built
with a naive `grep -rhoE 'crate::[a-z_]+'` that **matched doc comments and test code**. This pass
strips `//` comments and truncates each file at its first `#[cfg(test)]` before grepping.

### Sequence

1. `grep -n 'changeset::\|use crate::changeset' worktree.rs` and the reverse — verify the claimed
   `changeset ↔ worktree` cycle.
2. `grep -n 'worktree' changeset.rs` — all occurrences, then production-only.
3. Rebuild the whole `tddy-core` module map with
   `awk '/^[[:space:]]*#\[cfg\(test\)\]/{exit} {print}' | sed 's://.*::'` before the grep.
4. Per-cycle symbol-level greps: `stream↔backend`, `backend↔toolcall`, `backend↔workflow`,
   `workflow↔presenter`, `changeset↔workflow`.
5. `grep -rn 'pub struct ClarificationQuestion\|pub enum WorkflowEvent\|pub struct GoalId\|…'` —
   locate every cycle-causing DTO.
6. `wc -l tddy-workflow/src/*.rs` + its `[dependencies]` — size the existing shared crate.
7. Leaf test on each DTO's own file — what `crate::` paths it needs to move.
8. `pr_stack/mod.rs` production-only symbol census.
9. `grep -rln 'tddy_workflow_recipes::github_pr\|github_rest_common'` outside the crate.

### Grep / glob — notable hits

- `worktree.rs:11` → `use crate::changeset::{read_changeset, write_changeset, BranchWorktreeIntent};`
  plus two **test-only** uses at 1721/1822.
- `changeset.rs` production lines contain **no** `crate::worktree` path. The only hit is
  `changeset.rs:473`, inside a doc comment: ``/// … (matches [`crate::worktree`] conventions).``
- `stream/mod.rs:9` → `use crate::backend::{ClarificationQuestion, QuestionOption};` — the **whole**
  `stream → backend` edge.
- `toolcall/client_wire.rs` → `crate::backend::QuestionOption` — the **whole** `toolcall → backend` edge.
- `backend/mod.rs:230-231` → `pub use crate::workflow::ids::GoalId;` and
  `pub use crate::workflow::recipe::{GoalHints, PermissionHint, WorkflowRecipe};` — a **facade**,
  plus one real use at `:451`.
- `workflow/ → backend`: `write_codex_thread_id_file` ×2, `RemoteToolEnv`, `AgentOutputSink`.
- `workflow/ → presenter`: `crate::presenter::WorkflowEvent` ×2 — the whole edge.
- `changeset.rs → workflow`: `context::Context`, `ids::`, `recipe::WorkflowRecipe` — three symbols.
- `workflow/ → changeset`: `crate::changeset::Changeset` ×1 — the whole edge.
- DTO definitions: `ClarificationQuestion` `backend/mod.rs:510`, `QuestionOption` `backend/mod.rs:523`,
  `ProgressEvent` `stream/mod.rs:14`, `WorkflowEvent` `presenter/events.rs:17`,
  `GoalId` `workflow/ids.rs:10`, `WorkflowRecipe` `workflow/recipe.rs:41`.
- **Leaf test:** `workflow/ids.rs` → *no* `crate::` paths. `presenter/events.rs` → *no* `crate::` paths.
  `ClarificationQuestion`/`QuestionOption` → plain serde structs, no `crate::` paths.
  `workflow/recipe.rs` → **not** a leaf (`backend`, `changeset::Changeset`, `presenter::WorkflowEvent`,
  `workflow::{context,graph,hooks,ids}`).
- `tddy-workflow` crate: **384 lines total**, only `artifact_paths.rs` + a 10-line `lib.rs`, and its
  `[dependencies]` contain **no `tddy-*` crate at all**. Both `tddy-core` and
  `tddy-workflow-recipes` already depend on it.
- `pr_stack/mod.rs` production census: `tddy_core::changeset::Stack` ×15, `read_changeset` ×7,
  `update_stack_atomic` ×3, `tddy_core::worktree::*` ×4, `crate::orchestrate_pr_stack::github` ×6,
  `git_ops` ×4 — and exactly **one** reference each to `workflow::{task,recipe,ids,hooks,graph}`
  and `backend` (those are the `PrStackRecipe` impl at lines 131–418).
- `github_pr` is consumed **outside the crate** by `tddy-tools/src/server.rs`.

### Findings — two corrections to Exploration 1

1. **There is no `changeset ↔ worktree` cycle.** The edge is one-way (`worktree → changeset`); the
   apparent back-edge was a doc-comment link. Breaking it is therefore not design work — splitting
   `worktree.rs` is a plain mechanical extraction.
2. **`backend/` is not a free crate extraction.** It sits in a six-module strongly-connected
   component with `stream`, `toolcall`, `workflow`, `changeset` and `presenter`.

3. **Every one of the five real cycles is caused by a shared DTO living inside a behaviour module**,
   and each edge is one to four symbols wide:

   | Cycle | The thin edge | Break |
   |---|---|---|
   | `backend ↔ stream` | `stream/mod.rs:9` needs `ClarificationQuestion`, `QuestionOption` | move the two DTOs |
   | `backend ↔ toolcall` | `toolcall/client_wire.rs` needs `QuestionOption` | move the two DTOs |
   | `presenter ↔ workflow` | `workflow/` needs `WorkflowEvent` ×2 | move `presenter/events.rs` |
   | `backend ↔ workflow` | `backend/mod.rs:230-231` re-exports `GoalId` + the recipe trio | move `workflow/ids.rs`; retire the facade |
   | `changeset ↔ workflow` | `workflow/` needs `Changeset` ×1; `changeset` needs `Context`, `ids`, `WorkflowRecipe` | move `workflow/ids.rs`; the rest needs `WorkflowRecipe` relocated, which is **not** a leaf move |

4. **`tddy-workflow` is an existing, near-empty, zero-`tddy-*`-dependency crate that both affected
   crates already depend on** — the natural home for the moved vocabulary. Moving
   `workflow/ids.rs` (107 lines), `presenter/events.rs` (40 lines) and the two question DTOs into
   it — roughly **160 lines, all with zero `crate::` dependencies** — breaks **three of the five
   cycles outright** and thins a fourth.

5. `tddy-pr-stack` is viable: `pr_stack/mod.rs`'s operations need `changeset::Stack`, four
   `worktree` helpers and the GitHub client — not the recipe machinery, which is confined to the
   `PrStackRecipe` impl at lines 131–418.

---

## Exploration 3 — what the restructure tooling can and cannot do for this stack

Run during `/plan-pr-stack` Step 1 after the user required that mechanical moves be planned as
`tddy-tools restructure` intents, with hand-written code as the residue.

### Sequence

1. Load the `code-restructuring` skill; read `references/plan-schema.md`.
2. `grep -rhoE '"[a-z_]{4,}"' packages/tddy-code-restructuring/src/*.rs` — the op names the executor
   actually knows, versus the ones the skill documents.
3. `grep -rn 'move_module_to_crate' packages/tddy-code-restructuring/src/` — is the cross-crate op real?
4. Read the project memory note on `move_module_to_crate`'s measured hit rate.
5. `grep -rn 'fn source_crate_of' -A 30 packages/tddy-code-restructuring/src/` — is the nested-module
   limitation still in the code?
6. `grep -n 'cycle' packages/tddy-code-restructuring/src/crate_move.rs` — is the facade/cycle refusal
   still wired?
7. Read `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`.

### Findings

**The op table in the skill's `plan-schema.md` is incomplete.** It documents no cross-crate
operation, but `move_module_to_crate` exists in `packages/tddy-code-restructuring/src/crate_move.rs`
and is validated in `plan.rs`. Rust's supported set is therefore: `extract_module` (with `to_file`),
`extract_module_to_file`, `extract_method`, `extract_variable`, `extract_trait`, `inline_method`,
`rename_symbol`, `move_module_to_crate`. **`move_symbol` and `move_file` are TypeScript-only — Rust
has no whole-symbol move.**

**Three refusals confirmed still present in the code, not merely historical:**

| Refusal | Evidence | Consequence for this stack |
|---|---|---|
| **Flat modules only** | `crate_move.rs:773` `source_crate_of` still does `strip_suffix("/src/{module}.rs")` | `session_catalog/`, `backend/`, `session_actions/`, `pr_stack/`, `orchestrate_pr_stack/` are **all** directory-shaped and refused before rust-analyzer spawns |
| **Origin facades read as cycles** | `crate_move.rs:246` `refuse_a_dependency_cycle` | every `pub use` re-export left for back-compat must be re-pointed **by hand first** |
| **Journal is repo-scoped** | `.restructure/journal.jsonl`; a completed plan blocks the next | each layer of a multi-plan node needs `.restructure/` archived by hand between plans |

Measured hit rate from the previous stack: `move_module_to_crate` moved **13 of 21** modules on
`#unbundle` node 1 and **0 of ~24** on node 2. There is also **no `create_file` operation**, by
design — so a destination crate's skeleton is always hand-written.

### The consequence: every crate-extraction node is phased, not two-part

The limitation and the capability compose into one repeatable four-phase template, because
`extract_module --to_file` produces exactly the flat `<crate>/src/<module>.rs` shape that
`move_module_to_crate` requires:

| Phase | Kind | What |
|---|---|---|
| **A** | mechanical | `extract_module` + `to_file` — carve the seam out of the god-file into a **flat, crate-root-declared** module. Composes many seams in one plan |
| **B** | manual | destination crate skeleton (`Cargo.toml`, `lib.rs`, workspace member); re-point origin `pub use` facades that would trip the cycle refusal |
| **C** | mechanical | `move_module_to_crate` on the now-flat module — the precondition Phase A manufactured |
| **D** | manual | residue: dependency declarations, facade tidy-up, the `pub use <crate>::*;` duplicates the op appends per-operation |

Nodes that stay **inside** one crate (`presenter-split`, the file splits in `telegram` and
`recipe-parsers`) are Phase A only, and are the cheapest work in the stack.

**Budget honestly:** Phase C is the phase with a measured history of refusing. Each node's changeset
states the `move_module_to_crate` attempt, and names `git mv` as the recorded fallback (renames
preserve blame) rather than treating tool success as the plan of record.

---

## Exploration 4 — node-specific: the `Presenter` field clusters and the `changeset.rs` seams

Run while writing this node's PRD. It produced **two corrections to Exploration 1's figures**.

### Sequence

1. `awk 'NR>=55 && NR<=136' presenter_impl.rs | grep -cE '^    [a-z_]+:'` — exact field count.
2. The same with `-oE` — the field names in declaration order.
3. `awk 'NR>=152 && NR<=1788' presenter_impl.rs | grep -cE '^    (pub )?(async )?fn '` — method
   count **inside the production impl only**.
4. `grep -n 'pub struct Stack\|pub struct StackNode\|impl StackNode\|impl Stack\|pub struct Changeset' changeset.rs`
   — the two data models' boundaries.

### Corrections to Exploration 1

| Figure | Exploration 1 said | Measured | Why it was wrong |
|---|---|---|---|
| `Presenter` fields | 44 | **37** | eyeballed from a 80-line struct read, not counted |
| `Presenter` methods | 79 | **46** | `grep -c` ran over the **whole file**, counting the test module's `fn`s |
| `presenter_impl.rs` prod lines | 1,788 | 1,788 | unchanged, correct |

The conclusion is unaffected — a 37-field struct with 46 methods in 1,788 production lines is still a
god object — but the sub-struct grouping in FR3 had to be built from the real field list.

### The 37 fields, in declaration order

```
state  workflow_event_rx  answer_tx  workflow_backend  workflow_output_dir
workflow_session_dir  workflow_conversation_output  workflow_debug_output  workflow_debug
workflow_result  pending_questions  current_question_index  collected_answers
agent_output_buffer  agent_output_partial_row_active  awaiting_open_answer  workflow_handle
broadcast_tx  intent_tx  tool_call_rx  pending_tool_call_response  workflow_socket_path
workflow_worktree_dir  backend_selection_pending  deferred_backend_factory
pending_workflow_start  deferred_cli_model  workflow_recipe  recipe_slash_selection_pending
recipe_resolver  start_slash_structured_run_active  critical_state  tddy_data_dir
agent_activity_dir  agent_activity_worktree  agent_activity_source  agent_activity_pending
```

**The clusters are not inferred — the struct's own doc comments name them.** `agent_activity_*`
carries a four-line comment about provenance and the checkout the agent edits; `broadcast_tx`,
`intent_tx` and `tool_call_rx` each say what external surface they serve; `deferred_*` and
`*_pending` say they exist for the deferred-start path. FR3's five sub-structs are those groups made
explicit, and two fields — `state` and `tddy_data_dir` — belong to none of them and stay on
`Presenter`.

### `changeset.rs` boundaries

| Line | Item | Seam |
|---:|---|---|
| 17 | `ClarificationQa` | stays (with `QuestionOptionForQa`) |
| 44 | `pub struct Stack` | `changeset/stack.rs` |
| 53 | `pub struct StackNode` | ″ |
| 91 | `impl StackNode` | ″ |
| 101 | `impl Stack` | ″ — runs to ~277 |
| 278 | `pub struct Changeset` | `changeset/model.rs` |
| 352 | `ChangesetState` | ″ |
| 417 | `ChangesetWorkflow` | ″ |
| 555+ | `read_changeset`, `write_changeset`, `write_changeset_atomic`, `ensure_changeset_recipe` | `changeset/io.rs` |
| 617+ | `merge_persisted_workflow_into_context`, `merge_post_workflow_into_context` | `changeset/merge.rs` |

**Two unrelated data models share this file.** `Stack`/`StackNode` are the PR-stack model that
`#carve` 9/9 needs as its own module; `Changeset` is the session model. Nothing in the first names
the second. Moving a whole `impl` is free of caller churn — a method is reached through its type — so
`impl Stack` and `impl StackNode` carry no rewrite cost.

### Why Phase C needs `#carve` 1/9

Of the three modules Phase C moves into `tddy-workflow`:

- `tddy-core/src/workflow/ids.rs` — **nested**, refused by `source_crate_of` today
- `tddy-core/src/presenter/events.rs` — **nested**, likewise
- `backend_questions` — flat only *because* Phase A manufactures it

and all three leave a `pub use` facade in `tddy-core`, which is the shape that trips
`refuse_a_dependency_cycle`. Both of `#carve` 1/9's fixes are therefore on this node's path, which is
what makes it wave 2 rather than wave 1.

### Findings

**`workflow/recipe.rs` is not a leaf, and that is why `backend/` stays.** It needs `crate::backend`,
`crate::changeset::Changeset`, `crate::presenter::WorkflowEvent` and four `workflow::` submodules
(`context`, `graph`, `hooks`, `ids`). So `WorkflowRecipe` cannot move with the other DTOs, the
`backend ↔ workflow` edge survives as the recipe trio, and extracting `backend/` to its own crate is
out of reach in this node. Filed as a todo rather than attempted.
