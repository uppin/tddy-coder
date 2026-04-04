# Clean-code analysis: TDD interview implementation

**Scope:** `packages/tddy-workflow-recipes/src/tdd/interview.rs`, interview-related paths in `hooks.rs`, `mod.rs`, `graph.rs`, compared briefly to `packages/tddy-workflow-recipes/src/grill_me/`.

---

## Summary

The TDD **interview** step is implemented with a **small, focused relay module** (`interview.rs`) that mirrors the **grill-me** pattern of persisting elicitation across hook boundaries via `.workflow/` files. **Recipe and graph wiring** are consistent: `interview` is the start goal, edges `interview → plan`, and `before_plan` applies the staged handoff into `answers` so **plan** can consume it after `after_task` clears context keys.

**Overall quality:** Good separation between **prompt/handoff I/O** (`interview.rs`) and **orchestration** (`hooks.rs`). The main structural cost is that **`hooks.rs` remains a large “god” module** for the whole TDD workflow (not introduced solely by interview). Naming is mostly coherent; a few opportunities exist to align terminology with grill-me and to reduce duplicated “session bootstrap” logic.

---

## What is good

### Module boundaries and SRP

- **`interview.rs`** owns a single concern: paths, system/user prompts, writing the relay file, and loading it into `Context` as `answers`. It does not parse PRD or touch changeset session lists—appropriate **Single Responsibility**.
- **`graph.rs`** only adds the `interview` node and edge; **`mod.rs`** centralizes recipe metadata (states, `goal_ids`, `start_goal`, hints, submit policy). Clear **separation of graph vs recipe policy**.

### Naming consistency

- Task id **`interview`** matches `GoalId`, hook branches, and display name **“Interview”**.
- Relay path **`INTERVIEW_HANDOFF_RELATIVE`** / `interview_handoff_path` read clearly; **`persist_*`** / **`apply_staged_*`** express direction (write after interview vs read before plan).
- **`TDD_INTERVIEW_GRAPH_HANDOFF_VERSION`** documents that graph/handoff semantics can evolve together (useful for integration tests or migration).

### Function size and complexity

- **`interview.rs`**: functions are short; control flow is linear (existence checks, trim, set context).
- **`before_interview`** / **`after_interview`**: manageable; `after_interview` delegates persistence to `interview::persist_interview_handoff_for_plan`.

### Documentation and comments

- Module-level `//!` docs in `interview.rs` explain **why** the relay exists (hooks clear `answers` after the task).
- **`elicitation_after_task`** comment correctly states PRD approval runs after **plan**, not interview—avoids confusion with grill-style elicitation.
- **`graph.rs`** documents full topology and conditional edges; interview addition is reflected in doc comments.

### Comparison to grill-me (patterns)

| Aspect | Grill-me | TDD interview |
|--------|----------|----------------|
| Elicitation phase | `grill` | `interview` |
| Follow-on “write brief/plan” | `create-plan` | `plan` (PlanTask) |
| `.workflow/` relay | `grill_ask_answers.txt` (read **after** `grill`, then delete) | `tdd_interview_handoff.txt` (write **after** interview; read in `before_plan`) |
| `answers` → `prompt` | `before_task` on `grill` when resuming | `before_interview` when `answers` non-empty |
| Compose rich plan prompt | `compose_create_plan_user_prompt` | Handoff → `answers` then PlanTask / planning pipeline |

The **directional** difference (grill loads socket relay into `answers` after the step; TDD persists agent output to disk for the **next** step) matches the engine rule that **`after_task` clears `answers`/`prompt`**, so the TDD design is **consistent with that contract**.

---

## Issues and suggestions

### 1. Duplication vs grill-me (conceptual, not necessarily code-shared)

- **`before_interview`** repeats the same **“if `answers` then move to `prompt` and clear `answers`”** structure as `GrillMeWorkflowHooks::before_task` for `"grill"`. Extracting a shared helper (e.g. `transfer_answers_to_prompt_if_nonempty(context)`) would reduce drift risk—**optional** and only if you want cross-recipe consistency in one crate.

### 2. Duplication inside `hooks.rs` (interview-adjacent)

- **Session id allocation** (`Uuid::now_v7`, empty-check) appears in `before_interview`, `before_acceptance_tests`, `before_red`, etc. Not interview-only, but **`before_interview` adds another copy**. A private `ensure_session_id(context)` would shrink noise (non-breaking refactor).

### 3. `hooks.rs` size and cohesion

- **`RunnerHooks` for TDD** is ~1100+ lines handling every task. Interview adds a bounded amount, but the file is still hard to navigate. **Splitting by phase** (`hooks/interview.rs`, `hooks/plan.rs`, …) or by `before_*` / `after_*` would improve maintainability without changing behavior.

### 4. Logging style mix

- Interview paths use structured `target: "tddy_workflow_recipes::tdd::hooks"` / `tddy_workflow_recipes::tdd::interview`; elsewhere hooks still use `log::debug!("[tdd hooks] ...")`. **Harmonizing** log targets improves filterability (cosmetic).

### 5. Relay file lifecycle

- Grill-me **deletes** `grill_ask_answers.txt` after load. TDD **does not delete** `tdd_interview_handoff.txt` after `apply_staged_interview_handoff_to_plan_context`. That may be intentional (audit/retry), but if you want parity and to avoid stale replays, consider **removing the relay after successful staging**—only if product requirements agree (behavior change; document and test).

### 6. `TddRecipe` fallback state (`mod.rs`)

- **`next_goal_for_state_inner`** uses `_ => Some(GoalId::new("interview"))` as default. Powerful for unknown states, but **masks typos** in state strings. Prefer explicit listing or logging when hitting the fallback (optional hardening).

---

## Optional refactor ideas (non-breaking)

1. **Extract `transfer_clarification_from_answers_to_prompt(context)`** in a small `hooks` helper or shared `workflow_recipes::elicitation` module—same behavior, less duplication with grill-me.
2. **`ensure_context_session_id(context)`** helper used by `before_interview`, `before_acceptance_tests`, `before_red` (incremental, test-preserving).
3. **Unit test** for `before_interview` + `after_interview` + `before_plan` ordering (in addition to existing `tdd_interview_handoff_unit.rs` and acceptance tests)—guards against reordering in `after_task`.
4. **Align `TDD_INTERVIEW_GRAPH_HANDOFF_VERSION`** consumers: if nothing reads it yet, either wire it into a test assertion or add a one-line comment where it is (or will be) checked—avoids “dead constant” confusion.

---

## Conclusion

The TDD interview implementation is **clean at the feature boundary** (`interview.rs` + targeted hook wiring) and **aligned with grill-me’s mental model** (elicitation before plan, `.workflow/` relay, `answers`/`prompt` handoff). The main improvement area is **incremental decomposition of `hooks.rs`** and small **DRY** helpers for session/bootstrap logic shared across `before_*` handlers—not specific flaws in the interview design itself.
