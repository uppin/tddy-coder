# Analyze Clean Code — GitHub PR Tools

## Summary

The GitHub PR tools work is **coherent and readable**: `tddy-tools` centralizes REST constants, MCP tool names, a mock transport, and curl-based live calls; `tddy-workflow-recipes` layers prompt awareness for merge-pr and tdd-small. The main maintainability cost is **cross-crate duplication** of token resolution and GitHub header/curl patterns (already flagged in `plans/evaluation-report.md`), plus a **documentation/behavior mismatch** between merge-pr (token-gated awareness) and tdd-small merged red (always-on awareness sentence). `server.rs` remains a large “permission brain” with GitHub handlers as a small, well-localized addition. Tests split **fast unit checks** (in-module) from **acceptance** (integration tests), though live MCP/GitHub paths are intentionally not exercised end-to-end.

## Strengths

- **Naming and API surface (`github_pr.rs`)**  
  - Stable `GITHUB_*` constants, explicit MCP name constants, and `CreatePullRequestParams` / `UpdatePullRequestParams` keep REST shape obvious.  
  - `MockGithubTransport` + `RecordedHttpRequest` give a clear seam for tests without network I/O.  
  - `GithubPrError` variants separate auth from transport/API failures; user-facing messages avoid leaking token material.

- **MCP layer (`server.rs`)**  
  - GitHub tools are thin wrappers: deserialize → map to `CreatePullRequestParams` / `UpdatePullRequestParams` → call `*_via_rest_api` → JSON string. Low cyclomatic complexity per handler.  
  - `GithubCreatePullRequestToolInput` uses `schemars` descriptions consistently for agent-facing fields.

- **Workflow prompts (`hooks.rs`, `red.rs`)**  
  - Merge-pr prompts are decomposed (`analyze_system_prompt`, `sync_main_system_prompt`, `finalize_system_prompt`) with shared `SCOPE` and targeted unit tests for content.  
  - `merge_pr_github_tools_awareness_line` and the static `MERGE_PR_GITHUB_TOOLS_AWARENESS_AUTHENTICATED` separate “policy text” from wiring.

- **Comparison to `merge_pr/github.rs`**  
  - Same mental model: token from env → curl to `api.github.com` → parse JSON. Error strings in `github.rs` are merge-specific; `github_pr.rs` is PR create/update — separation by feature is reasonable.

- **Test organization**  
  - `github_pr.rs` `red_unit_tests`: headers, JSON body, token absence, MCP name list.  
  - `tests/github_pr_acceptance.rs`: PRD-oriented flows with env hygiene (`EnvUnsetGithubTokens`).  
  - `tests/github_tools_recipe_prompt_acceptance.rs`: recipe-level assertions for merge-pr line and tdd-small merged prompt.  
  - `hooks.rs` `mod tests`: prompt strings without needing a full workflow run.

## Issues (severity)

| Severity | Issue |
| -------- | ----- |
| **Medium** | **DRY:** `github_token_from_env` / equivalent logic appears in `packages/tddy-tools/src/github_pr.rs`, `packages/tddy-workflow-recipes/src/merge_pr/github.rs`, and `github_env_token_present()` in `hooks.rs`. Header triplets (`Accept`, `User-Agent`, `X-GitHub-Api-Version`) are repeated; User-Agent strings differ by design (`tddy-tools` vs `tddy-coder-workflow-recipes`) but version/Accept are the same literal in multiple places. |
| **Medium** | **Semantic consistency:** Merge-pr appends GitHub PR tool awareness only when `github_env_token_present()` is true. **tdd-small** `merged_red_system_prompt()` always appends the GitHub PR section while comments/`tdd_small_github_pr_tools_awareness_sentence` imply “when credentials may be available.” Acceptance tests assert inclusion unconditionally — behavior is internally consistent with tests but **not aligned** with merge-pr’s env gating (see `plans/evaluation-report.md` nuance). |
| **Low** | **`github_pr.rs`:** `create_pull_request` and `update_pull_request` duplicate the “read token → headers → build body → push” sequence; could be one private helper without changing behavior. |
| **Low** | **`server.rs`:** `GithubUpdatePullRequestToolInput` omits `schemars(description = "...")` on several fields where `GithubCreatePullRequestToolInput` documents them — minor inconsistency for MCP discoverability. |
| **Low** | **`server.rs`:** `get_info().with_instructions(...)` still describes only the permission prompt; GitHub PR tools are discoverable via tool descriptions but not in the server instruction blurb. |
| **Low** | **Operational coupling:** Live paths depend on `curl` on `PATH` (documented in tool description). Not a style issue, but it concentrates failure modes outside Rust types (same as `merge_pr/github.rs`). |
| **Info** | **Test gap:** No automated test that MCP registration lists `github_create_pull_request` / `github_update_pull_request` through the full rmcp surface (evaluation report: MCP list not introspected end-to-end). |

## Refactoring suggestions

1. **Shared GitHub env + constants (DRY, optional crate boundary)**  
   - Extract `fn github_token_from_env() -> Option<String>` and `GITHUB_API_VERSION` / `GITHUB_ACCEPT` (and optionally a small `curl` header builder) into a workspace crate depended on by both `tddy-tools` and `tddy-workflow-recipes`, *or* into the smallest existing shared crate if policy allows. Keeps merge-pr and tools identical without copy-paste.  
   - If a new crate is undesirable, document the duplication with a single comment pointer: “keep in sync with `merge_pr/github.rs` / `github_pr.rs`” to reduce drift risk.

2. **Unify tdd-small vs merge-pr gating (product decision)**  
   - Either gate tdd-small merged red awareness on the same env check as merge-pr, or update module/docs/tests to state that merged red **always** educates about MCP PR tools (safe generic text). Aligns user expectation and reduces “why two behaviors?” confusion.

3. **Small internal refactor in `github_pr.rs`**  
   - Extract `fn record_pr_request(transport, method, path, body)` or `require_github_token_for_mock()` to shrink duplicate blocks in `create_pull_request` / `update_pull_request`.

4. **MCP polish**  
   - Add `schemars` descriptions to `GithubUpdatePullRequestToolInput` fields matching create.  
   - Optionally extend `get_info` instructions with one sentence that GitHub PR tools exist when token is set (without duplicating full tool docs).

5. **Tests**  
   - If end-to-end MCP registration must be guaranteed, add a test that starts the handler/router metadata path and asserts tool names (may be heavy; acceptable as a follow-up).  
   - Keep acceptance tests as the contract for PRD; add focused unit tests if gating behavior for tdd-small changes.

---

*Scope: `packages/tddy-tools/src/github_pr.rs`, `packages/tddy-tools/src/server.rs`, `packages/tddy-workflow-recipes/src/merge_pr/hooks.rs`, `packages/tddy-workflow-recipes/src/tdd_small/red.rs`, comparison with `packages/tddy-workflow-recipes/src/merge_pr/github.rs`.*
