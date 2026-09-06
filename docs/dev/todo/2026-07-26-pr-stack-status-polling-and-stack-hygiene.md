# 2026-07-26 — PR-Stack — status polling and stack hygiene

**Category:** Future enhancement
**Source:** pr-stack-ux-recovery changeset, 2026-07-26

- **Manual verification against the live stack was never done** — orchestrator session
  `019f9dd5-716d-7071-96ac-464ff7b98c2a` on `uppin/tddy-coder`: confirm node `attach-store` recovers
  (its recorded session is gone, so the row must offer Start session pre-filled to resume
  `feature/session-attach-docs/attach-store`) and that PR #351 shows on
  `feature/session-attach-docs/attach-proto`. It needs the daemon **rebuilt and installed**,
  `auth_storage` (`/var/lib/tddy/auth`) **created and chowned to the daemon user**, and a **re-login**
  (the widened `read:user repo` scope). Every automated suite is green; this is the one unverified path,
  and it is also the end-to-end check that the token store and the `remote` leg work against a real
  repo rather than a fixture.
- **The repoint recovery was never run against a live stack** — deferred to after merge by the
  developer (source: `pr-stack-repoint-dead-end`, 2026-07-26). Exercise a genuinely stranded node — a
  predecessor whose PR merged on GitHub and whose branch was deleted, with the plan still recording
  `open` — and confirm the row offers "Repoint to `<default>`", that taking it drops the dead parent and
  leaves the node startable, and that a refusal shows its reason. Do it on **two** projects: one that
  stores `main_branch_ref` and one that does not. The second matters most: sending an empty target used
  to select a different rule server-side and silently do nothing, and no automated test covers that path
  end to end.
- **GitHub poll volume is one lookup per rendered branch per 5s** — `useQueryBranch` polls every branch
  a row renders. Adding each node's *base* branch to the poll set costs nothing extra: a base is by
  definition some node's own `branch` and was already in the set (`resolvedBranches` is deduplicated).
  The rate-limit problem was entirely the **two hooks polling the same fact** — `usePrStatus` and
  `useQueryBranch` both reaching the same authenticated `GET /pulls` — which is fixed by removing
  `usePrStatus`. What remains is a fixed 5s interval per branch: a ten-node stack is ~2 calls/second,
  which is comfortable against a 5000/hour user limit but still linear in stack size and unaffected by
  nothing having changed. Batch the resolution into one call per stack, cache per branch with ETags, or
  back off when the response is unchanged.
- **Dangling `session_id` links are never scrubbed** — `DeleteSession` leaves the deleted session's id
  on the orchestrator's stack node; the orphan state is derived at render instead. A periodic (or
  delete-time) reconciliation would make the stored stack match reality, which matters for anything
  reading the changeset directly rather than through the web.
- **`origin/<branch>` freshness depends on the last fetch** — the start-blocked warning reads the
  local remote-tracking ref, so a branch pushed from another machine reads as missing until this host
  fetches. A periodic background fetch, or a fetch-on-demand from the row, would close the gap. Since
  `pr-stack-repoint-dead-end` this also has a **destructive** consequence, not just a delaying one: the
  row will offer "Repoint to `<default>`" for a base that is actually alive, and taking it drops the
  parent edge from the plan for good.
- **Two checked-in generations of `connection.proto` disagree** — `packages/tddy-rust-typescript-tests/gen/connection_pb.ts` predates `RepointPlannedPr` entirely, while
  `packages/tddy-web/src/gen/connection_pb.ts` is kept current. Regenerating the former is a large diff
  unrelated to any one changeset, which is why it keeps being skipped. Source: `pr-stack-repoint-dead-end`.
- **A repoint names its target but not what it drops** — the control reads "Repoint to `<target>`" and
  collapses the node onto that single parent, dropping every other edge (intended, D18). The operator is
  never shown *which* predecessors that removes, and there is no undo. Naming them in the button's
  tooltip, or a confirm step when more than one edge would go, would make the cost visible.
  Source: `pr-stack-repoint-dead-end`.
- **`repoint_planned_pr_node` has three pre-existing silent-failure paths** — a failed `git rev-parse`
  collapses to an empty `expected_sha`, which git reads as "the remote ref must not exist" and turns a
  `--force-with-lease` into a guaranteed rejection; `merge_base` failure invents `effective_base`; and a
  force-push failure is only `log::warn!`, so the RPC returns success while `origin/<branch>` still points
  at the old base and the PR was re-targeted anyway. Untouched by `pr-stack-repoint-dead-end`, which only
  moved them inside a branch guard. Source: `pr-stack-repoint-dead-end`.
- **A `tddy-coder`-embedded web server retains no GitHub token** — `packages/tddy-coder/src/run.rs`
  builds its own `AuthServiceImpl` without a `GitHubTokenStore`, so PR status there reads
  *unavailable* even after a real login. The daemon path is wired; this one is not.
- **`start_sandboxed_cursor_cli_session` takes no `stack_parent`** and calls neither
  `resolve_chain_base_ref` nor the node link, so a cursor-cli session cannot be a PR-stack child at
  all. Wiring it is a larger gap than this changeset, not an oversight in it.
- **Nothing tests that `qualified_head` is *applied* at the two call sites** — only that the function
  itself is correct. `get_open_pr` / `get_pr_by_head` reach `api.github.com` through free `curl`
  helpers with no injection seam, so pinning the outgoing `head` value would need either a network
  call or an HTTP-transport abstraction. Worth adding the seam if that module grows.
