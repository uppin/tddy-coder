# Refactoring plan (post-validate)

Consolidated from `validate-tests-report.md`, `validate-prod-ready-report.md`, and `analyze-clean-code-report.md` for the **review workflow recipe** branch.

## Priority 1 — Production / integration

1. **Wire `review.md` on real submit** — Invoke `persist_review_md_from_branch_review_json` / `persist_review_md_to_session_dir` from the live `tddy-tools submit` relay or engine path when `branch-review` validates, with a resolved `session_dir`, so sessions persist `review.md` without relying on tests only.
2. **UTF-8 safe truncation** — Replace raw byte slicing in diff truncation with boundary-safe truncation (e.g. `floor_char_boundary` on `str`) to avoid panics on multibyte boundaries.
3. **Bound `git diff --stat` / prompt size** — Cap stat output and diff material for large repos (memory / prompt bloat).

## Priority 2 — Tests

4. **Policy assertion** — Add a test that `recipe_should_skip_session_document_approval("review")` matches grill-me-style expectations (parity with existing policy tables).
5. **Negative JSON** — Expand `branch-review` rejection cases (wrong `goal`, empty body) mirroring `post-green-review` patterns.
6. **Optional E2E** — If product requires it, one path through daemon/coder that asserts `review.md` on disk after submit.

## Priority 3 — Clean code

7. **Extract shared hook assembly** — Deduplicate `inspect` / `branch-review` branches in `review/hooks.rs` (`before_task`).
8. **Single source of truth for merge-base docs** — Align `prompt::merge_base_strategy_documentation()` with `git_context` implementation (one module owns the narrative).
9. **Goal ID constants** — Replace repeated `"inspect"` / `"branch-review"` string literals with `const` or a small internal enum at boundaries.
10. **Hook unit tests** — Add focused tests for `ReviewWorkflowHooks` prompt assembly (parity with grill-me hook tests).

## Repository hygiene

11. **Do not commit** root-level `.red-submit-payload.json`, `.green-submit-payload.json`, `.evaluate-submit-payload.json`, or similar — `.gitignore` or delete before merge.
