# Analyze Clean Code Report

## Overall assessment

The new **review** workflow code is **coherent and aligned** with existing recipe patterns (notably **grill-me**): a small `mod.rs` recipe surface, dedicated `hooks` / `prompt` modules, and focused helpers for git context, parsing, and persistence. Modules are **appropriately scoped** for a single workflow. The main gaps are **localized duplication** in hooks, **light test coverage** next to grill-me’s hook tests, and **documentation drift risk** between merge-base behavior in `prompt.rs` vs `git_context.rs`. Overall quality is **good for a first vertical slice**, with clear, prioritized improvements.

## Strengths

- **Naming and layout**: `ReviewRecipe`, `ReviewWorkflowHooks`, `inspect` / `branch-review` goals, `REVIEW_MD_BASENAME`, and file names match the rest of `tddy-workflow-recipes` (compare `GrillMeRecipe`, `GrillMeWorkflowHooks`, `GRILL_ME_BRIEF_BASENAME`).
- **Separation of concerns**: `parse.rs` owns schema-shaped validation; `persist.rs` owns I/O; `git_context.rs` owns git subprocess details; `prompt.rs` owns operator/agent-facing text. This is a **solid Single-Responsibility** split.
- **Recipe module (`mod.rs`)**: Mirrors grill-me’s structure (graph build, hooks factory, goal hints, manifest). **`SessionArtifactManifest`** is implemented consistently with a single known artifact.
- **Structured submit path**: `BranchReviewOutput` with `#[serde(deny_unknown_fields)]` plus explicit `goal` and `review_body_markdown` checks keeps invalid payloads out early.
- **Daemon touchpoint**: `telegram_session_control.rs` is minimal and **correctly documented** as aligning CLI normalization; adding `"review"` to the extended keyboard list is consistent with product naming.

## Issues / improvements (prioritized)

1. **Duplication in `hooks.rs` (`before_task`)**  
   The `"inspect"` and `"branch-review"` arms both: resolve `repo_root`, build `git_block`, append the same section header `"## Branch changes (deterministic scope)\n\n"`, and set `system_prompt`. Extracting something like `fn build_system_prompt_for_task(task_id: &str) -> String` (or building `git_block` once and passing into shared `fn append_branch_scope(prompt_fn, git_block)`) would **reduce drift** if one arm later diverges by mistake.

2. **Merge-base story split across two modules**  
   `prompt::merge_base_strategy_documentation()` and `git_context::merge_base_commit_for_review()` must stay in sync. Today they agree (ordered refs, then `HEAD`), but **future edits** could diverge silently. Options: reference one canonical doc string from the other, or move the strategy description next to the implementation and have the prompt import/surface it (single source of truth).

3. **`git_context.rs`: helper inconsistency**  
   `git_output()` exists for strict success paths, but `format_diff_context_for_prompt` uses raw `Command` and **treats non-success as soft errors** in the prompt (parenthetical messages). That is intentional for UX, but the split means **two styles** of git invocation. Consider a small internal `fn git_output_relaxed(...)` or a comment block explaining why stat/diff intentionally differ from `git_output`.

4. **Tests vs grill-me**  
   `grill_me/hooks.rs` includes **unit tests** for hook behavior (`after_task` file relay). `review/hooks.rs` has **no corresponding tests** for deterministic prompt assembly or repo resolution edge cases. Adding focused tests (even with mocked/minimal paths) would match project TDD expectations and guard regressions.

5. **Stringly goal IDs**  
   `"inspect"` and `"branch-review"` appear many times across `mod.rs` and `hooks.rs`. **Private `const` goal name strings** (or a tiny enum used only at boundaries) would reduce typo risk and ease refactors.

6. **`persist.rs` logging**  
   The log line uses `out.review_body_markdown.len()` while the file is normalized with `trim_end_matches` and a trailing newline; **character length vs UTF-8 byte length** can differ from “bytes written”. Prefer logging after write, or use `std::fs::metadata` if exact size matters.

7. **`tddy-tools/src/review_persist.rs`**  
   This is a **thin wrapper** over `persist_review_md_to_session_dir` with an extra log target. Acceptable as a **stable API boundary** for the tools binary, but if it never grows, consider documenting that intent (one line) so it is not mistaken for duplication to delete hastily.

## Suggestions

- **Extract shared prompt assembly** in `hooks.rs` to remove the duplicate inspect/branch-review block and keep event emission in one place.
- **Single source of truth** for merge-base documentation: either generate operator text from the same list used in `merge_base_commit_for_review`, or centralize a `const MERGE_BASE_CANDIDATE_REFS: &[&str]` used by both docs and loop.
- **Add hook tests** at least for: context key `system_prompt` contains the git section header when a fake temp repo is used, or pure string composition tests without git if easier.
- **Use `const` goal names** in `review` module for readability and consistency with other recipes as they evolve.
- **`telegram_session_control.rs`**: No change required for clean code; keep the file’s comment in sync when CLI recipe lists change elsewhere.

---

**Path written:** `plans/analyze-clean-code-report.md`

**Top 3 issues (one line):** (1) Duplicate `inspect`/`branch-review` prompt assembly in `hooks.rs` — extract helper; (2) Merge-base behavior documented in `prompt.rs` and implemented in `git_context.rs` — consolidate single source of truth; (3) Review hooks lack unit tests compared to grill-me — add coverage for prompt/context behavior.
