# Validate-tests report — GitHub PR tools refactor

## Executive summary

Full workspace tests were run with `./dev cargo test --workspace`. **All tests passed** (exit code 0). **No failures.** Aggregate **1,207** tests reported as passed across **169** separate `test result` summaries (Cargo does not emit a single workspace total). **8** tests were **ignored** (expected skips, including one doc-test). Wall clock **~3 min 22 s** (~202 s).

Coverage for the GitHub PR tools scope in `plans/evaluation-report.md` is **largely exercised** via unit and acceptance tests in `tddy-tools` and `tddy-workflow-recipes`. **Remaining gaps** match the evaluation report: no end-to-end **rmcp** tool-list introspection, no automated **live curl** GitHub REST path, and **merge-pr vs tdd-small** env-gating behavior differs in nuance (documented in review, not fully asserted by tests).

---

## Commands run

| Command | Notes |
|---------|--------|
| `./dev cargo test --workspace --no-fail-fast` | From repo root `/var/tddy/Code/tddy-coder/.worktrees/tddy-tools-github-pr-tools`. Full workspace per project convention. |

Cargo emitted: `warning: Git tree '...' is dirty` (informational; does not affect test outcome).

---

## Results

### Overall

| Metric | Value |
|--------|--------|
| Exit code | **0** (success) |
| Failed tests | **0** |
| Passed (sum of per-target `test result` lines) | **1,207** |
| Ignored (sum across targets) | **8** |
| Approx. duration | **~202 s** |

### Failures

**None.** No `test result` line reported a non-zero `failed` count.

### Feature-scoped crates (high signal)

**`tddy-tools` — GitHub PR / changeset-related tests**

| Target | Passed | Failed | Notes |
|--------|--------|--------|--------|
| `src/lib.rs` unittests (incl. `github_pr::red_unit_tests`, schema) | 9 | 0 | Mock transport, headers, JSON bodies, MCP name list helper |
| `src/main.rs` unittests (`server::tests`) | 12 | 0 | Approval prompt; does **not** assert GitHub PR tools on rmcp surface |
| `tests/github_pr_acceptance.rs` | 4 | 0 | Auth rejection, create/update payloads, `registered_github_pr_mcp_tool_names()` |
| `tests/persist_changeset_github_tools_acceptance.rs` | 1 | 0 | `changeset-workflow` + `github_pr_tools_metadata` validates |

**`tddy-workflow-recipes`**

| Target | Passed | Failed | Notes |
|--------|--------|--------|--------|
| `src/lib.rs` unittests | 78 | 0 | Includes merge-pr hook unit tests |
| `tests/github_tools_recipe_prompt_acceptance.rs` | 2 | 0 | Merge-pr awareness line + tdd-small merged red prompt |

---

## Coverage assessment vs `plans/evaluation-report.md`

| Evaluation theme | Test evidence | Assessment |
|--------------------|---------------|--------------|
| Auth gating (`GITHUB_TOKEN` / `GH_TOKEN`) | `github_tools_reject_when_token_missing`, `github_pr::red_unit_tests::*` | **Covered** (mock + env) |
| Mock REST payloads / headers | `github_tools_create_*`, `update_*`, red unit tests | **Covered** |
| MCP tool names documented / listable | `mcp_server_lists_github_pr_tools`, constants vs `registered_github_pr_mcp_tool_names()` | **Covered** at helper level |
| Merge-pr prompts when authenticated | `merge_pr_hooks_prompt_mentions_github_pr_tools_when_authenticated`, merge_pr hook unit tests | **Covered** (prompt strings) |
| tdd-small merged red + awareness | `tdd_small_system_prompt_includes_github_pr_tools_awareness` | **Covered** |
| Optional `github_pr_tools_metadata` in changeset-workflow | `persist_changeset_workflow_still_validates_after_github_tools_change` | **Covered** |
| Runtime `curl` / live REST | Not invoked in tests (mock-only for HTTP) | **Not automated** (by design in evaluation) |
| rmcp server exposes same tool names as registration helper | No test drives `PermissionServer` / rmcp list-tools | **Gap** (per evaluation) |
| merge-pr vs tdd-small env gating nuance | merge-pr uses token-aware path in hooks; tdd-small tests always expect awareness in merged prompt | **Partially covered**; behavioral difference noted in evaluation, not regression-tested |

---

## Gaps and recommendations

1. **rmcp introspection (evaluation-report)**  
   Add an integration-style test that starts or inspects the MCP handler surface (e.g. list tools from `PermissionServer` / rmcp) and asserts `github_create_pull_request` and `github_update_pull_request` appear, matching `registered_github_pr_mcp_tool_names()`. This closes the “helper vs server” drift risk.

2. **Live GitHub / curl path**  
   Keep **out of default CI** unless you add opt-in, hermetic fixtures (recorded HTTP) or a dedicated manual/smoke job with secrets. Document reliance on `curl` + PATH as in the evaluation report.

3. **merge-pr vs tdd-small gating**  
   If product intent is “awareness only when a token may exist,” align implementation and tests; if intent is “always teach the model about tools,” document that and adjust merge-pr or evaluation wording. Add a focused test only once the intended contract is fixed.

4. **Integration vs unit**  
   Current PR flows are **unit + acceptance** (mock transport, schema validation, string prompts). **End-to-end** MCP + subprocess + network remains a gap; recommendation above addresses the highest-value slice (tool registration parity).

---

*Generated by validate-tests subagent for refactor validation.*
