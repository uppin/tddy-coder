# Validate prod-ready: TDD interview changeset

**Scope:** Interview-before-plan flow, relay handoff, `session_id` behavior in `before_interview` / `before_acceptance_tests` / `before_red`, `changeset` resume logic (`skip_failed_resume_transition`, `start_goal_for_session_continue`), graph and CLI (`run.rs`).

**Sources reviewed:**  
`packages/tddy-workflow-recipes/src/tdd/interview.rs`, `hooks.rs`, `mod.rs`, `graph.rs`;  
`packages/tddy-core/src/changeset.rs`, `workflow/recipe.rs`;  
`packages/tddy-coder/src/run.rs` (relevant sections).

---

## Executive summary

The interview step is wired consistently: a single relay file under the session directory bridges interview output into **plan** after hooks clear context, and logging uses `log::` (not `println!`) in the new interview module. Session identity is preserved where the hooks explicitly keep an existing non-empty `session_id`, and new IDs are allocated only when missing—aligning with the “plan-mode Fresh vs resume” contract for later steps.

Main production concerns: (1) **plan refinement** in `run.rs` still resolves the goal via `recipe.start_goal()`, which is now **`interview`**, so “refinement” may re-run interview instead of **plan**—likely a behavioral regression for post-PRD feedback flows; (2) **ignored `write_changeset` failures** remain a pattern in hooks (pre-existing style but still a durability gap); (3) **relay file content** is user/agent text—same confidentiality model as other session artifacts, but **`log::info!` lines log byte counts and paths** (`interview.rs:46–50`, `83–86`), which can be noisy or sensitive in centralized logs.

Overall the graph change is a **linear extra node** (`graph.rs:133–145`)—negligible runtime overhead; handoff adds **bounded extra I/O** (one write after interview, one read in `before_plan` when the file exists).

---

## Strengths

- **Clear handoff contract:** Constant path `INTERVIEW_HANDOFF_RELATIVE` (`.workflow/tdd_interview_handoff.txt`) and documented purpose in `interview.rs:1–4`, `40–41`, `57–58`.
- **TUI-safe logging in interview:** `interview.rs` uses only `log::debug!` / `log::info!` with an explicit `target:`—no raw stdout in this module.
- **Errors propagate where it matters:** `persist_interview_handoff_for_plan` returns `std::io::Result` (`interview.rs:41–55`); `apply_staged_interview_handoff_to_plan_context` maps read errors to a boxed error string (`interview.rs:72–73`).
- **PRD approval gate unchanged in intent:** `elicitation_after_task` still runs only after **`plan`**, not after interview (`hooks.rs:1075–1078`).
- **Resume semantics:** `TddRecipe::skip_failed_resume_transition` special-cases trailing `Planning` → `plan` noise (`mod.rs:221–229`); `start_goal_for_session_continue` documents failed-resume walk and TDD fallback to **`plan`** when appropriate (`changeset.rs:199–252`).
- **Graph topology:** One additional task and edge (`graph.rs:76–81`, `145–145`); `TDD_INTERVIEW_GRAPH_HANDOFF_VERSION` documents the handoff milestone (`graph.rs:13–14`).
- **Interview without mandatory tool submit:** `goal_requires_tddy_tools_submit` is `false` for interview (`mod.rs:217–218`)—appropriate for conversational elicitation.
- **CLI escape hatch:** New TDD sessions allow `--goal plan` to skip interview (`run.rs:952–966`).

---

## Risks and findings

| Area | Finding | Location |
|------|---------|----------|
| **CLI / UX regression** | `run_plan_refinement` builds `plan_gid` from `recipe.start_goal()` and runs that goal (`run.rs:2496–2498`). For TDD, `start_goal` is now **`interview`**, not **`plan`**, so post-approval “plan refinement” may execute the **interview** goal instead of regenerating the PRD from `refinement_feedback`. Variable name suggests **plan**; behavior no longer matches. | `run.rs:2496–2498` |
| **Durability** | Multiple `let _ = write_changeset(...)` / `let _ = write_changeset` on init paths—failures to persist state are silent. Interview-related examples: `before_interview` state update (`hooks.rs:229–231`), `after_interview` (`hooks.rs:251–253`), `before_plan` init (`hooks.rs:159–173`). | `hooks.rs` (throughout) |
| **Logging sensitivity** | `log::info!` logs full relay path and byte length when writing/loading handoff. Useful for ops; may be undesirable if session paths or sizes are considered sensitive in shared logs. | `interview.rs:46–50`, `83–86` |
| **Relay file security** | Handoff stores full agent/user clarification text under `session_dir/.workflow/`. Same trust boundary as `PRD.md` and other session files; not world-readable by default beyond OS permissions on the session directory. No encryption—consistent with rest of workflow artifacts. | `interview.rs:11–15`, `41–55` |
| **Error handling: interview** | `after_interview` uses `handoff_snapshot` or `result.response` (`hooks.rs:246–249`); empty handoff still writes a file (possibly empty after trim in downstream—`persist` writes raw `text`). Empty relay is handled on plan load (`interview.rs:75–81`). | `hooks.rs:236–255`, `interview.rs:75–81` |
| **Mixed log styles** | `hooks.rs` mixes structured `target:` logs with legacy `"[tdd hooks]"` / `"[tddy-core]"` prefixes—operational consistency only, not a functional bug. | e.g. `hooks.rs:60`, `873`, `1095–1096` |
| **Performance** | Extra graph node and one file read/write on the interview→plan boundary; repeated `read_changeset` in hooks during transitions (existing pattern). No unbounded in-memory graph growth. | `graph.rs`, `hooks.rs` |

---

## Recommendations

1. **Fix or document plan refinement goal:** For `run_plan_refinement`, either run `GoalId::new("plan")` explicitly when the intent is to refine the PRD from `refinement_feedback`, or split “interview follow-up” vs “plan regeneration” and document which goal runs. At minimum, rename `plan_gid` and add a comment if `start_goal` is intentionally used for refinement (`run.rs:2496–2498`).
2. **Consider surfacing changeset write failures:** Replace silent `let _ = write_changeset` with logged errors (at least `log::warn!` / `log::error!`) on interview and plan transitions, without changing success semantics—improves debuggability in production.
3. **Tune log levels for handoff:** Downgrade path/byte `log::info!` in `interview.rs` to `debug` in production-hardening passes if log volume or sensitivity is a concern; keep one concise info line if audit trails require it.
4. **Operational:** Ensure backup and access policies for `~/.tddy/sessions/...` (or configured session root) cover `.workflow/tdd_interview_handoff.txt` like other session artifacts.

---

## Confirmation

Report written to:

`/var/tddy/Code/tddy-coder/.worktrees/tdd-interview/plans/validate-prod-ready-report.md`
