# Validate-tests report

## Context

This run validates the **workflow free-prompting + approval policy** refactor described in [evaluation-report.md](./evaluation-report.md): `tddy-workflow-recipes` (resolver, `free-prompting` recipe, `approval_policy`), presenter bootstrap recording the recipe in the changeset (`tddy-core` `workflow_runner`), CLI/TUI allowlists (`tddy-coder`), gRPC terminal UTF-8 preview fix (`tddy-e2e`), and green/new-agent session contract (`tddy-integration-tests`).

Focus was **targeted `cargo test` invocations** via `./dev` (nix dev shell), not the full workspace `./test` suite.

## Commands run

All commands were executed from the workspace root  
`/var/tddy/Code/tddy-coder/.worktrees/workflow-free-prompting-approval`.

| # | Command | Exit code |
|---|---------|-----------|
| 1 | `./dev cargo test -p tddy-workflow-recipes` | **0** |
| 2 | `./dev cargo test -p tddy-coder --test presenter_integration --test cli_args` | **0** |
| 3 | `./dev cargo test -p tddy-e2e --test grpc_terminal_rpc` | **0** |
| 4 | `./dev cargo test -p tddy-integration-tests --test green_new_agent_session_contract` | **0** |

## Results summary

| Package / target | Outcome | Tests (passed / failed / ignored) |
|-------------------|---------|-------------------------------------|
| `tddy-workflow-recipes` | **PASS** | **55** passed (44 lib + 3 `goals_contract` + 2 `proto_goal_files` + 1 `proto_workflow_contracts` + 3 `recipe_policy_red` + 2 `workflow_recipe_acceptance`); 0 failed |
| `tddy-coder` `cli_args` | **PASS** | **10** passed; 0 failed |
| `tddy-coder` `presenter_integration` | **PASS** | **18** passed; 0 failed (runtime ~84.5s for the binary) |
| `tddy-e2e` `grpc_terminal_rpc` | **PASS** | **9** passed; 0 failed (~7.3s) |
| `tddy-integration-tests` `green_new_agent_session_contract` | **PASS** | **3** passed; 0 failed |

**Overall:** All four command invocations exited **0**. **95** test cases executed across these targets (55 + 10 + 18 + 9 + 3, noting workflow-recipes counts all integration test binaries in one `cargo test -p` run).

## Failures

**None.** No failing tests; no non-zero exit codes from the commands above.

## Coverage gaps / recommendations

### Aligned with PRD / evaluation-report gaps

1. **Product documentation (A4 / F5-adjacent)**  
   The evaluation report notes **`packages/*/docs/workflow-recipes.md` (or equivalent) is not updated** in the diff. Automated tests do not replace user-facing docs for `free-prompting`, resolver errors, or approval behavior.

2. **`approval_policy` vs `recipe_resolve` sync**  
   Policy helpers and the resolver/CLI name lists must stay consistent. There is **no compile-time guarantee** that `approval_policy`, `unknown_workflow_recipe_error`, and manifest registration stay aligned—recommend treating changes to any of these as a single review unit and adding tests when new recipes are introduced.

3. **Demo / `DemoArgs` goals**  
   Evaluation calls out **`DemoArgs` goal list** possibly needing updates for `tddy-demo` if demos should offer `free-prompting`. This was **not exercised** by the commands run (no `tddy-demo`-specific test target was executed).

4. **`tddy-core` `workflow_runner` unit coverage**  
   `workflow_runner.rs` has **no in-file unit tests**; bootstrap/recipe persistence is validated **indirectly** via `presenter_integration` (and related flows). If regressions are a concern, consider narrow unit tests around the bootstrap write path—optional, not required for this validate run.

### What *was* covered well for the stated PRD slice

- **Free-prompting resolver + metadata:** `workflow_recipe_acceptance`, `recipe_resolve::tests::*`, `recipe_policy_red`.
- **Presenter / approval paths:** `presenter_integration` includes TDD plan session-document approval, free-prompting without document approval when disabled by recipe, and bugfix bootstrap/reproduce flows.
- **gRPC terminal / UTF-8:** full `grpc_terminal_rpc` file (9 tests) passed.
- **Green / new-agent session:** `green_new_agent_session_contract` (3 tests) passed.

### Hygiene (non-test)

- **Do not commit** stray artifacts such as `.tddy-workflow-recipes-red-test-output.txt` (called out in the evaluation report).

### Optional broader validation

- Full workspace **`./test`** or **`./verify`** was **not** run in this pass; use it before merge if policy requires full-suite green.
