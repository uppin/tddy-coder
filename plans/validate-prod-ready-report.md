# Validate Production Readiness Report

## Summary verdict

**Needs work** — The review recipe core (graph, hooks, merge-base strategy docs, JSON parsing, and `review.md` writer) is implemented with consistent `log::` usage and no `println!` in the reviewed Rust modules. However, **`persist_review_md_from_branch_review_json` / `persist_review_md_to_session_dir` are not referenced from the live `tddy-tools submit` relay or `tddy-core` toolcall listener path** (only library exports and tests). Until the presenter or engine invokes persistence on `branch-review` submit with a resolved `session_dir`, **`review.md` will not appear in real sessions** despite successful structured submit. Additionally, diff truncation uses a raw byte slice that can **panic on UTF-8 boundaries**, and **`git diff --stat` is not size-capped** (large repos → large prompt material / memory).

---

## Findings by category

### Error handling

| Finding | Location | Notes |
|--------|----------|--------|
| **Structured JSON errors propagate** | `packages/tddy-workflow-recipes/src/review/parse.rs:16–28` | `serde_json::from_str` and goal/body validation return `Result<_, String>` with clear messages. |
| **Filesystem errors propagate** | `packages/tddy-workflow-recipes/src/review/persist.rs:16–22` | `create_dir_all` and `write` map errors to strings including path display. |
| **`tddy-tools` wrapper surfaces same errors** | `packages/tddy-tools/src/review_persist.rs:8–17` | Thin wrapper; failures from `persist_review_md_to_session_dir` bubble as `Result<(), String>`. |
| **Hooks swallow git “soft” failures in prompt** | `packages/tddy-workflow-recipes/src/review/git_context.rs:87–120`, `hooks.rs:54–64` | `format_diff_context_for_prompt` embeds stderr/error text in the prompt instead of failing `before_task`. Workflow continues; operator sees degraded context. **By design** but worth knowing for support. |
| **`merge_base_commit_for_review` silent candidates** | `packages/tddy-workflow-recipes/src/review/git_context.rs:34–48,56–82` | Failed `merge-base` attempts return `None` and try the next ref; eventual fallback to `rev-parse HEAD` or literal `"HEAD"` with `log::warn!` only if even `rev-parse` fails (`81–82`). No hard error to the user. |
| **No integration error path for missing `review.md` write** | *Gap* | Persistence API exists; **no caller** in `packages/tddy-coder` / `packages/tddy-core` grep for `persist_review_md` or `review_persist`. Submit flow stores JSON via `store_submit_result` (`packages/tddy-core/src/toolcall/listener.rs:118–127`) without writing `review.md`. |

### Logging

| Finding | Location | Notes |
|--------|----------|--------|
| **Uses `log` crate, not stdout** | `packages/tddy-workflow-recipes/src/review/*.rs` | `log::debug!`, `log::info!`, `log::warn!` throughout `mod.rs`, `git_context.rs`, `hooks.rs`, `persist.rs`. **No `println!` / `eprintln!`** in these files (verified by search). |
| **Targeted logging in `tddy-tools`** | `packages/tddy-tools/src/review_persist.rs:12–16` | `log::info!(target: "tddy_tools::review_persist", ...)`. |
| **Potential log volume** | `packages/tddy-workflow-recipes/src/review/mod.rs:144–151` | `plain_goal_cli_output` logs full agent output at `info` level when present — large outputs could flood logs (recipe-wide concern). |

### Configuration

| Finding | Location | Notes |
|--------|----------|--------|
| **Merge-base strategy is code-defined, not env-driven** | `packages/tddy-workflow-recipes/src/review/git_context.rs:58–72`, `prompt.rs:6–16` | Fixed candidate order: `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master`, then HEAD fallback. **No env vars** in `review/` for override. |
| **Operator documentation string** | `packages/tddy-workflow-recipes/src/review/prompt.rs:13–16` | `merge_base_strategy_documentation()` matches implementation intent (deterministic order, empty diff vs hard fail). |
| **CLI session env elsewhere** | `packages/tddy-tools/src/cli.rs:400–404` | `set-session-context` uses `TDDY_SESSION_DIR` / `TDDY_WORKFLOW_SESSION_ID` — not specific to review, but session dir for artifacts is environment-established, not chosen by review JSON. |

### Security

| Finding | Location | Notes |
|--------|----------|--------|
| **No shell when invoking git** | `packages/tddy-workflow-recipes/src/review/git_context.rs:18–32,101–103` | `Command::new("git")` with argument array; **no** `sh -c`. Injects via malformed ref are constrained: merge-base refs are fixed literals; `merge_base` output comes from git or `"HEAD"`. |
| **Path traversal from submit JSON** | `packages/tddy-workflow-recipes/src/review/persist.rs:17–18` | Output path is `session_dir.join(REVIEW_MD_BASENAME)` with constant `review.md` (`prompt.rs:4`). **No user-controlled path segments** from JSON for the file path. |
| **Secrets in logs** | `hooks.rs`, `persist.rs`, `git_context.rs` | Logs include repo path, merge-base commit, session dir, byte counts — **not** submit JSON body by default in persist (only sizes/paths). Full output logging risk in `mod.rs:149–151` if agent pastes secrets. |
| **Schema rejects extra fields** | `packages/tddy-workflow-recipes/src/review/parse.rs:7–17`, `generated/tdd/branch-review.schema.json` | `deny_unknown_fields` + `additionalProperties: false` reduces surprise keys in structured submit. |

### Performance

| Finding | Location | Notes |
|--------|----------|--------|
| **Diff body truncated to 48k bytes** | `packages/tddy-workflow-recipes/src/review/git_context.rs:106–113` | Caps agent prompt size for the unified diff section. |
| **`git diff --stat` not truncated** | `packages/tddy-workflow-recipes/src/review/git_context.rs:88–98` | Entire stat output embedded in prompt — very large histories can produce **large strings** and memory use. |
| **Full stdout loaded before truncate** | `packages/tddy-workflow-recipes/src/review/git_context.rs:101–113` | `git diff` output is fully read into memory, then truncated — **peak memory** equals full diff size. |
| **UTF-8 hazard on truncation** | `packages/tddy-workflow-recipes/src/review/git_context.rs:109–110` | `&s[..MAX]` on `str` **panics** if `MAX` splits a multibyte character. Rare for ASCII diffs, but not impossible for binary paths / non-UTF-8 (lossy conversion can still yield long UTF-8). Prefer `s.floor_char_boundary(MAX)` or truncate by char. |

---

## Residual risks

1. **Presenter / engine wiring for `review.md`** — `tddy_tools::review_persist::persist_review_md_from_branch_review_json` must be called with the active workflow `session_dir` and the validated submit JSON when the goal is `branch-review`. Until that exists in the same place other goals persist artifacts, the feature is **incomplete for end-to-end production use** despite unit/acceptance tests that call `persist_review_md_to_session_dir` directly (`packages/tddy-workflow-recipes/tests/review_recipe_artifact_acceptance.rs`).

2. **Submit path without `TDDY_SOCKET`** — `packages/tddy-tools/src/cli.rs:161–165`: if `TDDY_SOCKET` is unset, `run_submit` prints success **without** relaying; persistence is not invoked here either. Operators relying on offline validation still would not get `review.md` unless separately wired.

3. **Deterministic merge-base may not match team’s integration branch** — No env override; forks using unusual default branches rely on elicitation or may see empty/surprising diffs when fallbacks apply (`git_context.rs:74–82`).

4. **Binary / encoding edge cases** — Lossy UTF-8 from `from_utf8_lossy` plus byte slicing truncation (see performance) — potential panic or garbled truncation.

---

*Validation method: direct read of `packages/tddy-workflow-recipes/src/review/*.rs`, `packages/tddy-tools/src/review_persist.rs`, and cross-package grep for call sites / logging patterns.*
