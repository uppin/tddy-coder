# 2026-07-30 — PR-Stack — full control follow-ups

**Category:** Future enhancement
**Source:** pr-stack-full-control changeset, 2026-07-30

- **Split `pr_stack/mod.rs` (2434 lines) into `mod.rs` + `mutations.rs`** — the recipe definition (`PR_STACK_TOOL_NAMES`, the orchestrate prompt, `PrStackRecipe`, the two trait impls) and the 816-line stack-mutation API (11 functions + 4 input/output structs) are two modules in everything but name, and `pr_stack/` already has submodules (`bridge`, `hooks`). The production-code split costs **nothing**: all public functions keep their paths through a `pub use`, and the three private helpers are used only by their own group so they need no visibility change. The only real work is partitioning the single flat 1252-line `mod tests` — already banner-sectioned by subject, but ~1250 lines of churn, which is why it hasn't happened. It gets more expensive every changeset: the test module grew 547 lines in `pr-stack-full-control` alone. Bonus: 7 of the 9 mutation functions open with a function-local `use tddy_core::changeset::{read_changeset, update_stack_atomic};` that a module-level `use` would delete.
- **Split `orchestrate_pr_stack/github.rs` (1298 lines) into `github/{mod,lifecycle,insight}.rs`** — this is the file `pr-stack-full-control` is responsible for (549 → 1298, +546 production lines). The boundary is unusually clean: the two trait impls share **no** code beyond `resolve_token`/`require_token`, which would become `pub(super)`. `owner_repo_from_remote_url` is genuinely misfiled and belongs with the git helpers. The obstacle is that `mod tests` straddles the boundary — it holds `MockGithubPrApi` + `GithubPrApi` object-safety tests *and* the 14 `search_qualifiers` cases under one `use super::*`, so it has to be split in two and repointed, plus `mod real_impl_tests` moves to `lifecycle.rs`. No macro constraint blocks it.
- **`server.rs` (2527 lines): lift `server/subagent.rs` out first** — the subagent group (~350 lines: `subagent_enabled` … `subagent_tool_router`, the session table, the accounting file) is self-contained, needs no visibility changes, and has no coupling to the tool router. The larger win is a `server/pr_stack_tools.rs` holding the 15 PR-stack tool methods behind a second `#[tool_router(router = pr_stack_tool_router)]` — `ToolRouter` implements `Add` and `PermissionServer::new` already merges four routers, so the macro does **not** force one file. But it needs all 15 methods to become `pub(crate)` for `call_tool_by_name`, and both `advertised_tool_defs()` and `PermissionServer::new` to merge the second router — two edits where a miss is silent (the tool vanishes from the web Inspector but still dispatches by name, or the reverse).
- **A `PrSearchState` enum** — the search-state vocabulary (`open`/`closed`/`merged`/`all`) is written out by hand in five places: the `search_qualifiers` match arms and their `is:` counterparts, the `PrSearchQuery` doc, the `pr_search` schema description, and the tool description. `PrState` cannot serve (it has no `All`). An enum with `FromStr` + `as_qualifier()` would collapse the match, the validation error and the schema text into one definition. Related: `pr_search`'s default state is the bare literal `"open"` in the tool layer while the same tool's limit defaults route through the named `DEFAULT_SEARCH_LIMIT`/`MAX_SEARCH_LIMIT`.
- **The candidate-stack clone-push-wrap is still duplicated between the two appenders** — `add_planned_pr_node` and `adopt_pr_as_stack_node` each build `Stack { version, nodes: candidate_nodes }` inline before calling the shared `reject_if_cyclic`. A `stack_with(&existing, node)` helper would fold it; deliberately left out of the clean-code pass to keep that refactor scoped to the `topo_order` tail.

- **A testable transport seam for `RealGithubPrApi`** — `github_api_url` (`github_rest_common.rs:36`)
  hardcodes `https://api.github.com` and the transport is `Command::new("curl")`, so no HTTP mock can
  intercept it. Every GitHub request/response body in the repo therefore ships with zero automated
  coverage, and `pr-stack-full-control` adds eight more (`GET /pulls/{n}`, `/files`, `/reviews`,
  `/comments`, `/issues/{n}/comments`, `/commits/{sha}/check-runs`, `/search/issues`, and a title/body
  `PATCH`). `wiremock` is already a dev-dependency of both `tddy-tools` and `tddy-workflow-recipes`.
  Adding a base-URL override plus a real HTTP client would make the request shapes and the JSON parsing
  testable in one move — deliberately kept out of the feature changeset because it is a transport
  migration touching every existing call path.
- **Review-thread resolution state needs GraphQL** — `pr_comments` returns threads without a `resolved`
  flag because the REST API does not expose one (it is `reviewThreads.isResolved` on the GraphQL v4
  schema only). No field is emitted rather than guessing. Adding it means the first GraphQL call in the
  repository.
- **`pr_search` returns no branch names and does not paginate** — `GET /search/issues` omits a PR's head
  and base, so `base:` works as a query qualifier but the agent must follow up with `pr_read` to learn
  the branches; and `search_prs` fetches a single page (limit capped at 100). Following `Link` headers,
  or resolving each hit through `GET /pulls/{n}`, would close both gaps at a cost in API calls.
- **No gRPC/web surface for update, delete, set-parents or adopt** — `pr-stack-full-control` is
  agent-only by decision, so the web keeps `AddPlannedPr` / `RepointPlannedPr` / `GetPrStatus` /
  `QueryBranch` while the agent now has strictly more. Bringing the four new operations to
  `connection.proto` and to `PlannedPrRow`'s action set would remove the asymmetry — and `PrStackScreen`
  is where an operator most naturally wants "delete this row" and "rename this row".
- **`pr_delete_planned` leaves the branch, the worktree and the child session behind** — deletion is a
  plan operation by decision; it reports the orphaned `branch` and `session_id` and stops. An opt-in
  cleanup (close the PR, delete the remote branch, remove the worktree, delete the child session) would
  make "abandon this node" one step instead of four. Related: *dangling `session_id` links are never
  scrubbed*, below.
- **`AddPlannedPrInput.child_recipe` is still inert** — accepted by the Rust struct and present in
  `connection.proto:1122`, but `StackNode` has no field to carry it, so it is discarded on every path
  (`pr_stack/mod.rs:377-381`), and the MCP schema does not even expose it. Either give `StackNode` the
  field and honour it when spawning the child, or delete the parameter from both the struct and the
  proto. Untouched by `pr-stack-full-control`.
- **`Stack::topo_order` still treats an unknown parent id as a no-op** — in-degree is counted only over
  parents that resolve to a node (`changeset.rs:105`), so a dangling parent reference is silently
  ignored and no validation ever rejects a persisted `Stack` that holds one. Only `validate_stack_plan`
  rejects dangling parents, and it runs on plan *input*, never on what is on disk.
  `pr-stack-full-control` works around this by validating the candidate stack in every new writer and by
  making delete reparent rather than orphan, but the underlying model still cannot represent
  "this stack is invalid".
- **`update_stack_atomic` takes no lock** — it is read-modify-write plus an atomic rename, so two
  concurrent writers are last-writer-wins on the whole changeset. Every writer works around it
  individually by computing inside the closure. With seven more mutating tools calling it, an advisory
  file lock (or a single serialized stack-writer) becomes worth the change.
