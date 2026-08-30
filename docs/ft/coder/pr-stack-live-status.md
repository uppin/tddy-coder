# PR-Stack live status & repoint

**Product area:** coder (PR stacking) + web (PR-Stack Chat Screen)
**Related:** [PR stacking](pr-stacking.md), [Session drawer § PR-Stack Chat Screen](../web/session-drawer.md#per-workflow-session-views-the-pr-stack-chat-screen)

## Summary

Today a Planned PR in the PR-Stack Chat Screen is only loosely connected to the work it
represents: the `branch` is populated after a worktree exists, the child session link
(`session_id`) is set by the orchestrator agent, and the GitHub PR number/state is only
refreshed when the orchestrator agent runs an assess pass. Operators looking at the stack
cannot reliably tell, at a glance, *which* branch a Planned PR owns, whether a session is
already working it, what its PR number/link is, or whether it needs re-pointing after a
predecessor merged.

This feature makes the **remote branch name the durable link** between a Planned PR, its
worktree/session, and its GitHub PR, and surfaces live status directly in the web view —
independent of whether the orchestrator agent is running:

1. **Definitive branch on materialization.** A Planned PR carries a canonical `branch_suggestion`
   from creation, and records it as its `branch` the moment a child worktree actually creates that
   branch. `branch` therefore means "a branch that exists", and is the single join key used for
   every downstream lookup; the suggestion is a planned name only.
2. **Branch → session resolution (in-progress).** The PR-Stack view resolves the child session
   for a node by matching the node's branch against each session's branch, and marks the node
   *in progress* when a live session owns that branch. *(Amended 2026-07-26 — a node whose recorded
   child session no longer resolves is **orphaned**, not in progress: it offers **Start session**
   again, pre-filled to resume the branch it already owns. See
   [Orphaned-node recovery](#orphaned-node-recovery-added-2026-07-26).)*
3. **Branch → GitHub PR status (number, link, state).** The view queries GitHub for the PR whose
   head is the node's branch and shows the PR number as a link plus its state
   (open / merged / closed / draft). Status is polled on an interval so it updates without user
   action. *(Amended 2026-07-26 — the lookup is authenticated with the **operator's own GitHub
   token** from their web login, the `head` filter is qualified as `owner:branch`, and a lookup that
   cannot be performed reads **"PR status unavailable"** instead of silently claiming no PR exists.
   See [Authenticated PR status](#authenticated-pr-status-added-2026-07-26).)*
4. **Repoint / restack control.** When a node's predecessor has already merged, the row offers a
   Repoint control that drops the merged parent, rebases the node's local branch onto the new
   effective base, and re-targets the open GitHub PR's base branch. *(Amended 2026-07-26 — Repoint is
   offered for **any** unresolvable base, not only a merged predecessor, and reads **"Repoint to
   `<default branch>`"** when that is where it lands; a node that owns no branch is repointed as a
   plan-only edit. See [Repointing a dead-end planned PR](#repointing-a-dead-end-planned-pr-added-2026-07-26).)*
5. **Sequence-respecting base at spawn.** When a session is started for a planned node, its
   worktree is branched off the node's parent branch (the effective base, skipping merged
   ancestors) — not off the default branch. Starting a node whose non-merged parent owns no branch
   yet is refused, enforcing bottom-up ordering. The gate is the parent's *branch*, never its child
   session: a branch can be built on whether or not a session is still attached to it. *(Amended
   2026-07-26 — startability is now **shown, not discovered on failure**: `BranchResolution` carries a
   `remote` leg, and a node whose base branch is absent from `origin` shows a blocked **"Missing
   branch"** indicator in place of the Start-session button. See
   [Startability before the spawn](#startability-before-the-spawn-added-2026-07-26). Further amended
   2026-07-26 — the indicator no longer **replaces** the row's contents: a blocked row keeps its full
   information and its Start-session button beside a warning naming the issue. See
   [Repointing a dead-end planned PR](#repointing-a-dead-end-planned-pr-added-2026-07-26). Further
   amended 2026-08-30 — that button is no longer **disabled** either: a blocker is advice, not a
   refusal, so it stays pressable in a warning colour with an alert icon, and the daemon's own gate
   is what refuses a spawn that is genuinely impossible (D42). See
   [Cross-host planned PRs, a PR without a session, and force start](#cross-host-planned-prs-a-pr-without-a-session-and-force-start-added-2026-08-30).)*

## Current behavior being fixed (capability 5)

When a session is started for a planned node today — from either the web **Start session** button
or the orchestrator agent's `spawn-child` — the child worktree is branched off the project's
**default branch** (`origin/master`/`main`), regardless of the node's DAG parents:

- The web (`PrStackScreen.handleStartSession`) passes `stackParent = <orchestrator session>`.
- `resolve_chain_base_ref` (`connection_service.rs`) sees a pr-stack orchestrator parent —
  `parent_is_pr_stack_orchestrator` returns `true` — and short-circuits to `Ok(None)` ("an
  orchestrator has no branch of its own").
- With no chain base, the worktree is created from `project.main_branch_ref` (the default branch).

`Stack::effective_base_refs(node_id)` — which returns `origin/<nearest-non-merged-ancestor.branch>`
— already exists but is only consulted by the orchestrator's assess/repoint logic, never at spawn.
The node's `parents` are therefore ignored when creating the branch, so the stack sequence is not
respected.

## Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | `StackNode.branch` is recorded when a child worktree creates the branch; planning only sets `branch_suggestion` | The branch is the link key *and* the spawn gate — descendants base onto `origin/<branch>`. Pre-filling it from the suggestion would unblock a spawn onto a ref nothing created. The suggestion is the derivation source and the name the child is asked to create. |
| D2 | The web resolves the in-progress session by matching `node.branch` against `SessionEntry.branch`; a new `SessionEntry.branch` proto field carries it | Keeps session resolution in the frontend (no new "which session owns this branch" backend signal), reusing the sessions list the drawer already loads. |
| D3 | GitHub PR status comes from a new `GetPrStatus(branch)` RPC, polled on an interval | Live status without requiring the orchestrator agent to run; polling keeps the number/link/state fresh. |
| D4 | Repoint performs DAG-parent update **and** local-branch rebase **and** GitHub base re-target | Matches the orchestrator's existing repoint semantics (`bridge::execute_stack_repoint`) so a web-triggered repoint and an agent-triggered one converge. |
| D5 | The spawn-time base is resolved in the daemon (`resolve_chain_base_ref`), the single point both the web and agent spawn paths funnel through | One source of truth; the fix lands for both `Start session` and `spawn-child` at once. |
| D6 | Starting a node whose non-merged parent owns **no branch** is refused | Enforces bottom-up ordering: the parent's branch must exist to base onto it. Keyed on the branch, never on the parent's session — a closed or cleaned-up child session must not wedge the nodes below it. A merged parent is skipped, not required. |
| D7 *(2026-07-26)* — **amended by D40** | A node is **orphaned** when it records a `session_id` **and** its branch resolution has arrived with `session.exists = false` | Server-authoritative and already polled — `QueryBranch` scans sessions by changeset branch. Deriving it from the web's `sessions` list would misread a node as orphaned whenever its host is merely offline. Requiring the resolution to have *arrived* keeps the first render deterministic: an absent resolution is "unknown", never "orphaned". |
| D8 *(2026-07-26)* | Restarting an orphaned node that owns a branch pre-fills `work_on_selected_branch` with that branch | The branch, worktree and remote ref already exist, so `new_branch_from_base` would fail on "branch already exists". Resuming is also what the operator means — the work on that branch is not lost, only its session. |
| D9 *(2026-07-26)* | `BranchResolution` gains `BranchRemote { exists, sha }`, resolved server-side per branch | "Available to be based upon" is a fact about `origin`, and only the daemon can see the repo. Deriving it in the web from `Changeset.remote_pushed` or from a branch name being non-empty would report *available* for a local-only branch — exactly the case that fails inside `git fetch`. |
| D10 *(2026-07-26)* — **superseded by D16** | "Missing branch" **replaces** the Start-session button rather than disabling it | A disabled button with no explanation is the dead end operators already hit. The blocked indicator names the branch being waited on, so the next action (start the predecessor) is obvious. |
| D11 *(2026-07-26)* | PR lookups authenticate with the **operator's own GitHub access token**, retained daemon-side at login; the OAuth scope widens to `read:user repo` | The operator's own credential, already granted, and the only one that works for private repos and for future write operations. Chosen over an ambient `gh auth token` fallback (invisible state) and over embedding the token in the HMAC session token (which would put a live `repo` token into browser storage on a plain-http origin). Cost: an unavoidable re-login — see [Operator migration](#operator-migration-re-login-required). |
| D12 *(2026-07-26)* | A PR lookup that cannot be performed is reported as **unavailable**, never as "no PR" | The silent `Ok(None)` is why a live PR was invisible for a day. A stub/demo login is explicitly *not* a failure (D13). |
| D13 *(2026-07-26)* | **Stub / demo authentication resolves to "no PRs"** — never an error, never *unavailable* | `github.stub: true` exists so the product can be demoed and tested without real GitHub credentials. A stub login holds no access token by construction, so the lookup short-circuits to a clean empty result and must not fail the enclosing RPC — a demo must never surface an error banner or a red row. |
| D14 *(2026-07-26)* | A login that **cannot retain** its access token fails | Minting a session without its token is a half-login: the operator appears signed in while every GitHub-backed read reports itself unavailable, and re-authenticating — the one action that would fix it — is the one action they have no reason to attempt. Failing the exchange surfaces the real fault (an unwritable `auth_storage`) at the moment it is caused. Does not apply to a stub login (D13, stores nothing by construction) nor to an unconfigured store, which is a deliberate deployment choice. |
| D15 *(2026-07-26)* | `head` is qualified as `owner:branch` in both `get_open_pr` and `get_pr_by_head` | GitHub **ignores** an unqualified `head` and returns every PR, so `arr.first()` yields an arbitrary one (verified live: 30 PRs returned for a bare branch name vs 1 for `owner:branch`). Fixing only the display path would leave the orchestrator able to repoint or merge the wrong PR. |
| D16 *(2026-07-26)* — **reverses D10**, **amended by D42** | A blocked row keeps its **full information and its Start-session button (disabled)**, with a separate warning naming every blocking issue | D10 traded one dead end for another: the operator learned *why* the node was blocked but the row stopped showing what the node *was*, and there was still no action to take. The row is the only place a planned PR's title, description, planned branch, base and PR live; suppressing all of it to render one amber chip is strictly less information at the moment the operator most needs it. With Repoint beside it (D17) the disabled button is no longer a dead end — it is the thing that becomes enabled. |
| D17 *(2026-07-26)* | Repoint is offered whenever the node's base **cannot be resolved right now**, not only when a parent merged | "Merged predecessor" is one cause of a dead end among several — the base branch deleted after its PR merged, deleted without merging, or never pushed — and the plan's own `pr_status` is written by the orchestrator agent, so a merged predecessor frequently *is not* recorded as merged. Gating on the recorded phase is why the reported case (PR merged, branch deleted, plan still says open) had no recovery at all. All of these causes are indistinguishable to the operator and all are resolved identically: re-base onto the default branch. |
| D18 *(2026-07-26)* | The web computes the repoint **target** and **sends it** — `RepointPlannedPrRequest.target_base_branch` — and the daemon applies one rule: **retain exactly the parents that own that branch, drop the rest** | The button must promise exactly what the daemon will do, so the target has to be decided once, by the side that rendered the label. Having the daemon re-derive "which parents are dead" from git instead was rejected: `remote_branch_ref_sha` collapses every failure to `None`, so "absent from `origin`" and "could not tell" are indistinguishable, and the daemon would drop a real dependency whenever a probe failed or `repo_path` was not a working repo. The web is not inventing the fact either — it reads `BranchResolution.remote`, which the daemon itself resolved. **A repoint therefore collapses the node to a single parent** — the one owning the target, or none when no parent owns it. That is the intent, not a limitation of carrying one branch name: repointing is a decision to stack on one predecessor, so a multi-parent node comes out of it single-parent and its other edges are dropped. |
| D19 *(2026-07-26)* | Repointing a node that owns **no branch** is a **plan-only** edit — no rebase, no force-push, no PR re-target | `repoint_planned_pr_node` refused with `node '<id>' has no branch to repoint`, which rejected precisely the node this recovery exists for: planned, never started, wedged behind a base that no longer exists. A node with no branch has nothing to rebase and no PR to re-target; dropping the dead parents *is* the whole repoint. |
| D20 *(2026-07-26)* | The default-branch **name** reaches the row from `ProjectEntry.main_branch_ref`, the project list the drawer already loads — not a new RPC and not a live git probe | The label is rendered on every poll tick, and the authoritative resolver (`resolve_default_integration_base_ref`) runs `git fetch origin`. A legacy project with no stored default yields an empty name, in which case the button reads "Repoint to default branch" and **the daemon substitutes its own resolved default branch for the empty target**. That substitution is load-bearing, not a nicety: without it the empty string reaches the recipe as "no target named" and selects the drop-merged-parents rule, which in the dead-end case drops nothing and returns success against an unchanged plan — no error, no change. The recipe's no-target mode is in-process only. This is also what finally gives the Start-session dialog a named base for a root node (it passed `""`). |

## Orphaned-node recovery *(added 2026-07-26)*

`DeleteSession` removes a session directory and never touches any orchestrator's `Changeset.stack`, so
the node keeps a dangling `session_id`. Previously the row derived its mode from that field alone
(`isSpawned = Boolean(node.sessionId)`), so it showed a status chip forever and the planned PR became
unworkable with no recovery path.

- A node is **orphaned** when it records a `session_id` and its `QueryBranch` resolution has arrived
  with `session.exists = false` (D7). An absent resolution is *unknown*, not orphaned.
- An orphaned node offers **Start session** again. When it owns a `branch`, the dialog is pre-filled
  into `work_on_selected_branch` on that branch (D8) rather than asked to create it.
- The restarted session **re-links** to the node, so the recovery is durable: the daemon keys both the
  node link and chain-base resolution on the spawn's **effective branch** — see
  [PR stacking § Effective spawn branch](pr-stacking.md#effective-spawn-branch-added-2026-07-26).
- The dangling `session_id` is deliberately **left in the changeset**. Deriving the orphan state at
  render keeps `DeleteSession` free of a stack scan and self-heals across hosts; a restarted session
  repoints the link (last writer wins).

## Startability before the spawn *(added 2026-07-26)*

The spawn gate used to be pure changeset metadata: `Stack::base_ref_for_spawn` refuses a non-merged
branchless parent, but nothing checked git. A base branch absent from `origin` was caught much later
by `git fetch origin <branch>` inside worktree creation — *after* `StartSession` was accepted and the
session directory and changeset were written, leaving a broken session behind.

- `BranchResolution.remote` (`BranchRemote { exists, sha }`) is resolved per branch by
  `tddy_core::worktree::remote_branch_ref_sha` (`git rev-parse --verify --quiet
  refs/remotes/origin/<branch>`; every failure mode collapses to `None`, since it runs on the polled
  `QueryBranch` path).
- The PR-Stack view polls **each node's base branch as well as its own** — startability is a property
  of the *base*, and an unspawned node owns no branch to query.
- A node with no `session_id` whose base is unavailable is blocked. *(Superseded 2026-07-26 — it first
  rendered a **"Missing branch: `<base>`"** indicator **in place of** the Start-session button (D10);
  the row now keeps that button beside a warning naming every reason (D16), and since 2026-08-30
  keeps it **pressable** rather than disabled (D42).)* Three
  independent blockers produce it: a direct parent that is non-merged and owns no branch (the
  daemon's own gate), no ancestor owning a created branch at all, or a base branch whose `remote.exists`
  is `false`.
- A **root** node is always startable — its base is the project default branch, which exists by
  construction. A node that already **owns** a branch is never blocked either: its spawn *resumes* that
  branch, which creates nothing and fetches nothing.
- A base whose resolution has **not arrived** is *unknown*, never missing — blocking on it would be a
  permanent dead end of exactly the kind the indicator exists to remove.

## Repointing a dead-end planned PR *(added 2026-07-26)*

[Startability before the spawn](#startability-before-the-spawn-added-2026-07-26) made a dead end
*visible*; it did not make it *recoverable*. A planned PR whose predecessor's PR was merged and whose
branch was then deleted on `origin` read "Missing branch: `<deleted branch>`" forever: the base ref is
gone, so the row is blocked, and Repoint was offered only when the plan's own `pr_status.phase` said
`merged` — a field written by the orchestrator agent, which is frequently stale or was never run. The
row also **replaced** its own contents with the blocked indicator, so the operator lost the planned PR's
title, description, planned branch and PR link at exactly the moment they needed them.

**A blocked row is a full row.** Every planned PR renders its complete information regardless of
startability: title, description, the branch it owns or its planned branch, its base branch, its
worktree, and its PR link/state — every field it *has*. A blocked node necessarily owns no branch (a
node that owns one is never blocked, since its spawn resumes rather than creates), and branch is the
join key for the worktree and PR legs, so in practice a blocked row shows title, description, planned
branch and base branch. Nothing it has is suppressed; the point of D16 is that the row is not replaced. When a spawn is not currently possible the row adds a **warning** that
names each blocking issue, and its **Start-session button stays pressable** in the `warning` variant,
carrying an alert icon (`pr-stack-start-session-blocked-icon-<nodeId>`) and the same text as its
tooltip (D16 for the warning box; D42 for the button, which was disabled between 2026-07-26 and
2026-08-30). Nothing is hidden, and nothing is refused here: two of the three blockers are derived
from `QueryBranch` legs that can only see one host, so the view **advises** and the daemon's own gate
is what refuses an impossible spawn — with the real reason.

- Base branch absent from `origin` → *"Base branch `<base>` is not on origin"*.
- A direct parent that is non-merged and owns no branch → *"`<parent title>` has not created its branch
  yet"*.
- No ancestor owns a created branch at all → *"No predecessor owns a branch yet"*. Reported **only when
  no direct parent is the blocker**: a branchless non-merged direct parent already makes the base
  `no-ancestor-branch`, so naming both would state one fact twice. This blocker therefore appears only
  when the block is *above* a merged parent, which is the one case the parent-level message cannot
  express.

A dangling parent id is not a blocker — a plan referencing a node that does not exist is malformed, not
an unmet dependency, and the daemon's own gate likewise refuses only on a parent it can resolve.

**Repoint is the action beside it.** The Repoint control is offered whenever the base cannot be resolved
right now — *any* cause — as well as in its original merged-parent case (D17). It reads **"Repoint to
`<target>`"** so the operator knows where the node will land before clicking:

- The web computes the target by dropping every parent that cannot serve as a base right now — merged,
  branchless, or branch absent from `origin` per `BranchResolution.remote` — and then taking the nearest
  remaining ancestor's branch, or the project's default branch when none remains. In the reported case
  none remains, so the button reads "Repoint to `origin/master`".
- The default branch's *name* comes from `ProjectEntry.main_branch_ref` (D20). A legacy project that has
  none renders "Repoint to default branch"; the daemon still resolves the real ref when clicked.
- The target is **sent** with the click as `RepointPlannedPrRequest.target_base_branch` (D18), so the
  daemon does exactly what the label promised rather than re-deriving it from a git probe that cannot
  distinguish "absent" from "could not tell". An **empty** target on the wire means "the project's
  default branch", which the daemon substitutes from its own resolution — a client cannot always name
  it (D20).

**Repointing persists the new base in the plan.** Given a `target_base_branch`, the daemon retains
exactly the parents that own that branch and drops the rest, atomically — so the plan reflects reality
and every later read (the row, a spawn, `base_ref_for_spawn`, the orchestrator agent) agrees without
re-deriving anything. A target no parent owns means "detach": all parents are dropped and the node's base
collapses to the project default. The daemon rejects a target that names neither the resolved default
branch nor any parent's branch, so a stale label cannot silently rewrite the plan. An **empty** target
keeps the original behaviour (drop merged parents only) for any other caller.

For a node that already owns a branch the git effect is unchanged (rebase onto the new base, force-push
with lease, re-target the open PR's base). For a node that owns **no** branch the repoint is plan-only
(D19): there is nothing to rebase and no PR to re-target, and the node becomes startable on the next
render.

**A refused repoint says so.** The RPC can now reject — a stale label whose target names neither the
resolved default branch nor any parent's branch — and it can still fail for the reasons it always could
(the default branch is unresolvable, a rebase conflicts). The row shows the daemon's reason inline
(`pr-stack-repoint-error-<nodeId>`) and stays blocked, because nothing was persisted. A refusal the
operator cannot see would be a fresh instance of the dead end this feature exists to remove.

## Authenticated PR status *(added 2026-07-26)*

Previously the daemon read `GITHUB_TOKEN` / `GH_TOKEN` from its own process environment — unset under
systemd — and `get_pr_by_head` returned `Ok(None)`, indistinguishable from "no PR exists". A live PR
was therefore invisible. The token the operator had already granted was discarded at the end of
`ExchangeCode`.

- The GitHub OAuth authorize scope is **`read:user repo`**, and `AuthServiceImpl::exchange_code`
  retains the access token in a `GitHubTokenStore` (`put(login, token)` / `get(login)`). The daemon's
  `FileGitHubTokenStore` is rooted at the (previously unread) `auth_storage` config path.
- `ConnectionServiceImpl` resolves the **caller's own** token from their session-token login for
  `query_branch` / `get_pr_status`, and `RealGithubPrApi::with_token` takes it explicitly — it never
  falls back to the process environment, which would be a silent credential swap.
- `PrStatusView` gains `unavailable` + `unavailable_reason`: absent token for a real login, an
  insufficiently-scoped or expired token, a rate limit, or a transport error (D12). "No PR for this
  head branch" stays `exists = false`.
- A **stub/demo login** short-circuits to an empty result (`exists = false`, `unavailable = false`) —
  never an error, never *unavailable* (D13). Enforced through
  `GitHubOAuthProvider::issues_usable_access_token()`, which has **no default impl**, so every provider
  must state its answer rather than a test-environment branch deciding it.
- The `head` filter is qualified `owner:branch` in **both** `get_open_pr` and `get_pr_by_head` (D15).
- A failed lookup **degrades only the `pr` leg**: `QueryBranch` and `GetPrStatus` always succeed as
  RPCs, so the session / worktree / remote legs stay usable. `get_pr_by_head` drops its `Result`
  entirely — with no error channel, no caller can re-raise a failed lookup as `Status::internal`
  (previously a GitHub error propagated and the web's `.catch()` discarded the whole resolution).

### Operator migration: re-login required

Tokens minted before this change carry only `read:user`, so **every already-signed-in operator must
log out and log in again** before PR status works; a stored token with insufficient scope surfaces as
*unavailable* with a reason, not as "no PR". Because retention is now a hard login dependency (D14), a
configured `auth_storage` must be writable by the daemon user or the daemon **refuses to start**. See
[daemon § GitHub access-token retention](../daemon/session-auth.md#github-access-token-retention-added-2026-07-26).

## API surface

### Proto (`packages/tddy-service/proto/connection.proto`)

**`SessionEntry` — new field**

```proto
// The session's git branch (from Changeset.branch). Empty when the session has no branch yet.
// Lets the PR-Stack view resolve the in-progress child session for a planned node by branch.
string branch = 28;
```

**New shared message**

```proto
// Live GitHub PR status for one head branch. Surfaced on the PR-Stack Chat Screen.
message PrStatusView {
  // False when no PR (open, merged, or closed) exists for the queried head branch.
  bool exists = 1;
  uint64 number = 2;
  string url = 3;
  // "open" | "merged" | "closed" | "draft". Empty when exists = false.
  string state = 4;
  // Added 2026-07-26. True when the lookup could not be performed (no GitHub credential for this
  // login, insufficient scope, rate limit, transport error). Distinct from exists = false, which
  // means "no PR exists for this head branch". A stub/demo login is never unavailable (D13).
  bool unavailable = 5;
  // Operator-facing reason when unavailable = true; empty otherwise. Rendered as a tooltip.
  string unavailable_reason = 6;
}
```

**New RPC — `GetPrStatus`**

```proto
rpc GetPrStatus(GetPrStatusRequest) returns (GetPrStatusResponse);

message GetPrStatusRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session — resolves the repo (owner/repo) to query.
  string session_id = 2;
  // Head branch to look up (the planned PR's branch).
  string branch = 3;
}
message GetPrStatusResponse {
  PrStatusView status = 1;
}
```

**New RPC — `RepointPlannedPr`**

```proto
rpc RepointPlannedPr(RepointPlannedPrRequest) returns (RepointPlannedPrResponse);

message RepointPlannedPrRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session whose Changeset.stack holds the node.
  string session_id = 2;
  // The planned node to repoint (drop merged parents, rebase, re-target PR base).
  string node_id = 3;
  // The branch the node should be based onto after the repoint — the target the operator's
  // "Repoint to <target>" control named (added 2026-07-26, D18). The daemon retains exactly the
  // parents that own this branch and drops the rest; a target no parent owns means "detach", and the
  // node's base collapses to the project default. Rejected when it names neither the resolved default
  // branch nor any parent's branch. Empty keeps the original behaviour: drop merged parents only.
  string target_base_branch = 4;
}
message RepointPlannedPrResponse {
  // Updated JSON-serialized Stack, same wire shape as SessionEntry.stack_plan_json (field 23).
  string stack_plan_json = 1;
}
```

**New RPC — `QueryBranch`** *(added 2026-07-25)*

Resolves, for one head branch, the in-progress child **session**, its on-disk **worktree**, the
branch's **remote-tracking state**, and the live GitHub **PR status** in a single call. Added
**additively** — `QueryBranch` reuses `PrStatusView` for its `pr` field.

> **Updated 2026-07-26** — `QueryBranch` is now the PR-Stack view's **only** source of live branch and
> PR state. `GetPrStatus` is still served by the daemon (same handler path, same
> `pr_status_for_caller`) but **no longer called by the web**: both RPCs make the same authenticated
> `GET /pulls`, so polling both was two requests per branch per tick — ≈1440 requests/hour/branch
> against a 5000/hour user limit, exhausted within the hour on a five-node stack, after which every row
> read "PR status unavailable" permanently. `resolution.pr` is authoritative and arrives on the same
> tick, so the web's `usePrStatus` hook was removed.

```proto
rpc QueryBranch(QueryBranchRequest) returns (QueryBranchResponse);

message QueryBranchRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session — resolves the repo (owner/repo + repo_path) and the
  // sessions root to scan.
  string session_id = 2;
  // Head branch to resolve.
  string branch = 3;
}
message QueryBranchResponse {
  BranchResolution resolution = 1;
}

// Everything the PR-Stack row needs about one branch, resolved server-side by branch name.
message BranchResolution {
  string branch = 1;              // echoes the request; lets a response self-identify
  BranchSession session = 2;      // the in-progress child session working the branch
  BranchWorktree worktree = 3;    // the worktree checked out for the branch on disk
  PrStatusView pr = 4;            // live GitHub PR status (reuses PrStatusView)
  BranchRemote remote = 5;        // added 2026-07-26 — the branch on `origin`
}

// Added 2026-07-26. The remote-tracking state of a branch on `origin` — whether a descendant's
// worktree can be based onto it yet.
message BranchRemote {
  // False when `origin/<branch>` is absent; a descendant cannot be based onto it yet.
  bool exists = 1;
  // Commit the remote ref points at when exists = true; empty otherwise.
  string sha = 2;
}
message BranchSession {
  bool exists = 1;                // false when no session owns the branch
  string session_id = 2;
  bool is_active = 3;
  string status = 4;              // e.g. "active" | "idle"
}
message BranchWorktree {
  bool exists = 1;                // false when no worktree is checked out for the branch
  string path = 2;                // absolute worktree path when exists = true
}
```

The handler reuses the `get_pr_status` prologue (auth → os_user → sessions_base →
`require_pr_stack_orchestrator`) and composes: **PR** via `pr_status_for_caller` (the caller's own
retained token; unresolvable `owner/repo` → `exists = false`; no usable credential → `unavailable`;
never an error), **session** by scanning sessions whose `Changeset.branch == branch` (prefers active,
ties by most-recently-updated), **worktree** via `tddy_core::worktree::worktree_path_for_branch`, and
**remote** via `tddy_core::worktree::remote_branch_ref_sha` (added 2026-07-26).

### Rust (`tddy-core`, `tddy-workflow-recipes`, `tddy-daemon`)

- **`StackNode.branch` on materialization** — `pr_stack::add_planned_pr_node` and
  `plan_pr_stack::planned_prs_into_stack_nodes` leave `branch = None` and record the canonical name
  in `branch_suggestion`. `ConnectionServiceImpl::link_stack_node_to_spawned_branch` writes
  `branch` (plus `session_id`, as a fallback route back to the branch) once the child worktree has
  created it; a later session claiming the same branch repoints the fallback, last writer wins.
  `changeset::resolve_stack_node_branch` reads a node's branch, falling back to the branch recorded
  by its child session's changeset for a node linked before its branch was known.
- **`GithubPrApi::get_pr_by_head`** — new trait method returning the PR (open, merged, or closed)
  whose head matches a branch, with a derived `state`:

  ```rust
  pub struct PrView { pub number: u64, pub url: String, pub state: PrState }
  pub enum PrState { Open, Merged, Closed, Draft }
  fn get_pr_by_head(&self, head_branch: &str) -> Result<Option<PrView>, WorkflowError>;
  ```
- **`pr_stack::repoint_planned_pr_node`** — repoints a single node: drops merged parents from
  `node.parents` (persisted via `update_stack_atomic`), computes the effective base via
  `Stack::effective_base_refs`, rebases the node's local branch onto it, and calls
  `patch_pr_base` on the open PR. Reuses the `git_ops` + `github` primitives behind
  `bridge::execute_stack_repoint`. *(Revised 2026-07-26 — takes `target_base_branch: Option<&str>`.
  `Some(target)` retains exactly the parents that own `target` and drops the rest (D18); `None` keeps the
  original drop-merged-parents behaviour. A node with `branch = None` is a plan-only repoint — the
  rebase, force-push and `patch_pr_base` are skipped rather than erroring (D19).)*
- **Daemon handlers** — `get_pr_status` (resolve `owner/repo` from the orchestrator session's
  repo remote, call `get_pr_by_head`) and `repoint_planned_pr` (call
  `repoint_planned_pr_node`, return re-serialized `stack_plan_json`).
- **Repoint target validation (added 2026-07-26)** — `connection_service::validate_repoint_target(target,
  default_branch, parent_branches) -> Result<Option<String>, String>`, a pure helper in the same shape as
  `effective_spawn_branch`. Empty or whitespace-only → `Ok(None)` (the drop-merged-parents rule). The
  default branch is matched with `origin/` stripped from both sides, since the resolver returns a
  remote-tracking ref while a node's `branch` and a GitHub PR base are plain names. Anything else is an
  `invalid_argument` refusal — an unvalidated target would silently detach the node, because "no parent
  owns this branch" *is* the detach instruction.
- **Enrichment** — `session_list_enrichment` populates `SessionEntry.branch` from
  `Changeset.branch`.
- **Sequence-respecting base (capability 5)** — `resolve_chain_base_ref` (renamed/extended to
  accept the new branch name) resolves, for a pr-stack orchestrator parent, the stack node that
  owns `new_branch_name` (by `branch`, else by `branch_suggestion` for a node not yet materialized)
  and returns `Stack::effective_base_refs(node_id)`'s nearest non-merged ancestor ref (or the stack
  default when the node is a root; only branch-bearing parents contribute a ref). It first enforces
  the ordering guard: if a non-merged parent owns no `branch`, it errors (`failed_precondition`)
  with a message naming that parent. The guard never consults a parent's `session_id` — a closed or
  never-linked child session must not wedge a stack whose branch exists. Both spawn paths reach
  this via `spawn_claude_cli_session_inner`.
- **Remote-branch leg (added 2026-07-26)** — `tddy_core::worktree::remote_branch_ref_sha(repo_root,
  branch) -> Option<String>`, the public form of the private `remote_ref_exists`, backs
  `BranchResolution.remote`.
- **Effective spawn branch (added 2026-07-26)** — `connection_service::effective_spawn_branch(intent,
  new_branch_name, selected_branch_to_work_on)` returns `local_branch_name(selected_branch_to_work_on)`
  under `work_on_selected_branch` and the trimmed `new_branch_name` otherwise, and feeds the
  **node-link** sites in both spawn paths that take a `stack_parent`. `resolve_chain_base_ref`
  deliberately still keys on `new_branch_name`: under `work_on_selected_branch` a `Some(chain_base)`
  makes worktree setup run a real `fetch_chain_pr_integration_base`, which can fail and is pointless
  for a resume that creates nothing. See
  [PR stacking § Effective spawn branch](pr-stacking.md#effective-spawn-branch-added-2026-07-26).
- **GitHub credentials (added 2026-07-26)** — `tddy-github` widens the authorize scope to
  `read:user repo`, adds the `GitHubTokenStore` trait plus `AuthServiceImpl::with_token_store`, and
  gates retention on `GitHubOAuthProvider::issues_usable_access_token()` (no default impl, so every
  provider must state its answer instead of a test-environment branch deciding it). `tddy-daemon` adds
  `FileGitHubTokenStore` (rooted at `auth_storage`; `github-tokens.json` at mode `0600`, dir `0700`;
  `put` serialised on a process-wide mutex and published via `.tmp` + `fsync` + `rename`), a boot-time
  `probe_writable` in `build_auth_entries`, and `pr_status_for_caller`, which resolves the caller's
  token by login and returns a `PrStatusView` value rather than a `Status`.
  `tddy-workflow-recipes` adds `RealGithubPrApi::with_token` (never falling back to the process
  environment — an explicit-token instance authenticating as the host's ambient token would be a silent
  credential swap), `qualified_head`, and `PrLookupOutcome { Found | NotFound | Unavailable(reason) }`;
  `get_pr_by_head` drops its `Result` entirely.

### Web (`tddy-web`)

- `SessionEntry.branch`, `GetPrStatus*`, `RepointPlannedPr*`, `PrStatusView` regenerated into
  `gen/connection_pb.ts`.
- `resolveNodeSession(node, sessions)` — returns the live session whose `branch === node.branch`.
- `usePrStatus(client, sessionToken, orchestratorId, branches)` — polls `GetPrStatus` per branch
  on an interval, returns a `branch → PrStatusView` map.
- `PrStackScreen` gains a `sessions` prop (all sessions) threaded from `SessionsDrawerScreen`, and
  a `repointPlannedPr` handler.
- `PlannedPrRow` renders: an **in-progress** indicator (branch resolves to a live session), the
  **PR number as a link** + **PR state**, and a **Repoint** control when the node needs repoint
  (a predecessor merged).
- **`useQueryBranch(client, sessionToken, orchestratorId, branches)`** *(added 2026-07-25)* —
  per-branch polled, returning a `branch → BranchResolution` map. `PrStackScreen`
  threads it through `PlannedPrList` into `PlannedPrRow`, which now sources the **worktree** indicator
  (`pr-stack-worktree-<nodeId>`), **in-progress** badge (`pr-stack-session-<nodeId>`), and **PR**
  link/state from the `QueryBranch` resolution.
- *(2026-07-26)* `usePrStatus` is **removed** (no remaining caller) and the `prStatusByBranch` /
  `prStatus` props are off `PlannedPrList` / `PlannedPrRow`: `useQueryBranch` is the screen's single
  source. `PrStackScreen`'s poll set is `resolvedBranches` — the branches nodes own **plus** every
  node's base branch (deduplicated and sorted), because startability is a property of the base and an
  unspawned node owns no branch. Base branches add no volume of their own: a base is by definition some
  node's own `branch` and was already in the set.
- *(2026-07-26)* `isNodeOrphaned(node, resolution)` (pure module) decides the recovered state;
  `resolveStackBase(node, nodes) -> StackBase` (`default-branch` | `ancestor-branch` |
  `no-ancestor-branch`) and `branchlessNonMergedParent(node, nodes)` in `deriveStackBaseBranch.ts`
  decide startability — the discriminated and flat-direct-parent forms of the daemon's own gate, since
  the flattened label collapses a root, an all-merged chain and a chain with no ancestor branch to the
  same string while only the last is unstartable. `deriveStackBaseBranch` is now a thin wrapper over
  `resolveStackBase`, unchanged in behaviour.
- *(2026-07-26)* `CreateSessionInitialValues.selectedBranch` threads an existing branch into the
  dialog, honoured when `branchIntent === "work_on_selected_branch"`; `PlannedPrPanel` is the
  right-side docked/overlay container (see
  [session-drawer.md § PR-Stack Chat Screen](../web/session-drawer.md#pr-stack-chat-screen)).
- *(2026-07-26, dead-end recovery)* `startBlockers(node, nodes, branchResolutionByBranch) ->
  StartBlocker[]` (pure module, `startBlockers.ts`) returns every reason a node cannot be started, each
  with a human-readable `message`; the empty array means startable. It replaces the boolean
  `baseBranchMissing` / `baseBranch` pair, which could express only one reason and no text.
  `resolveRepointTarget(node, nodes, branchResolutionByBranch, defaultBranch) -> string` (same module)
  returns the branch a repoint would land on — the nearest ancestor branch surviving the drop of every
  unusable parent, else `defaultBranch` — and both names the Repoint control and is sent as
  `target_base_branch` (D18).
- *(2026-07-26, dead-end recovery)* `PrStackScreen` gains a `defaultBranch` prop, threaded from
  `SessionMainPane`'s already-loaded `projects` through `resolveWorkflowView`'s `WorkflowViewContext`
  (matched on `session.projectId` → `ProjectEntry.mainBranchRef`, D20). It feeds both the Repoint label
  and `baseBranchLabel`, which previously passed the empty string and left a root node's dialog with an
  unnamed base.
- *(2026-07-26, dead-end recovery)* `PlannedPrRow` always renders the row's full information and its
  Start-session button beside a warning (`pr-stack-start-warning-<nodeId>`) listing those blockers;
  the base branch gets its own line (`pr-stack-base-branch-<nodeId>`). The
  `pr-stack-missing-branch-<nodeId>` replacement indicator is **removed** (D16). *(Amended 2026-08-30,
  D42 — the button is no longer `disabled` when `startBlockers` is non-empty. It takes the `warning`
  variant and an alert icon, `pr-stack-start-session-blocked-icon-<nodeId>`, and stays pressable.)*
- *(2026-07-26, dead-end recovery)* `PrStackScreen.handleRepoint` records a per-node failure that the row
  renders as `pr-stack-repoint-error-<nodeId>`. It previously `await`ed the call with no `catch`, so a
  rejection was an unhandled promise and the row looked untouched — which the new `invalid_argument`
  refusal would have made reachable.

## Behavior and semantics

- **Branch as link key.** A node's `branch` is authoritative. Session resolution and PR lookup key
  off `node.branch`; `branch_suggestion` is only a derivation input, never the join key.
- **In-progress.** A node is *in progress* when some `SessionEntry.branch === node.branch` and that
  session is active. A node with no matching session shows its "Start session" CTA as today.
- **PR link/state.** When the resolution's `pr.exists = true`, the row shows `#<number>` linking to
  `url` and the `state`. When `exists = false`, no PR chip is shown; when `unavailable = true` the row
  reads "PR status unavailable" with `unavailable_reason` as a tooltip *(2026-07-26)*.
- **Row information is unconditional (2026-07-26, D16).** Every row renders the planned PR's full
  information — title, description, owned branch or planned branch, base branch, worktree, PR link/state
  and internal-status badge — whatever its startability. A row is never reduced to a single indicator.
- **CTA slot (mutually exclusive, revised 2026-07-26).** Exactly one of two things occupies a row's CTA
  slot, and a blocked row carries a warning beside it rather than in place of it:

  | Node condition | CTA slot shows | Warning |
  |---|---|---|
  | No `session_id`, base branch on `origin` (or node is a root) | **Start session** button, enabled | — |
  | No `session_id`, base branch absent from `origin` / unreachable | **Start session** button, **pressable**, `warning` variant + alert icon `pr-stack-start-session-blocked-icon-<nodeId>` *(D42)* | warning naming each blocking issue *(D16)* |
  | `session_id` set, resolution not yet arrived, or says a session exists | status chip (unchanged) | — |
  | `session_id` set, resolution says **no** session exists | **Start session** button, pre-filled to resume `node.branch` | — |

- **PR display (2026-07-26).**

  | Lookup outcome | Row shows |
  |---|---|
  | PR found | `#<number>` link + state chip |
  | No PR for this head branch | nothing |
  | Stub / demo login (D13) | nothing — identical to "no PR"; never an error |
  | Lookup unavailable | "PR status unavailable" with the reason as a tooltip |
- **Repoint availability** *(revised 2026-07-26, D17)*. The Repoint control appears when the node has at
  least one parent whose PR is merged (`StackNode::is_skipped` — the derived `needs-repoint` condition)
  **or** when the node's base cannot be resolved right now, for any cause.
- **Repoint label** *(2026-07-26, D18/D20)*. The control reads **"Repoint to `<target>`"**, where
  `<target>` is the nearest ancestor branch that survives dropping the unusable parents, else the
  project's default branch (`ProjectEntry.main_branch_ref`). With no stored default it reads "Repoint to
  default branch".
- **Repoint effect.** Repoint retains exactly the parents that own the requested
  `target_base_branch` and drops the rest *(2026-07-26, D18)*, persists that in the plan, rebases the
  local branch onto the effective base, force-pushes, and re-targets the open PR's base. A rebase
  conflict marks the node
  `pr_status.phase = "error"` (existing `execute_stack_repoint` behavior) and surfaces as an error. A
  node that owns **no** branch is repointed as a plan-only edit *(2026-07-26, D19)*: no rebase, no
  force-push, no PR re-target.
- **Spawn base.** A node with a single non-merged parent `n1` is branched off
  `origin/<n1.branch>`. A root node (no parents, or all parents merged) is branched off the stack
  default branch. Starting a node whose non-merged parent owns no branch is refused with a message
  naming the parent and its missing branch. Whether that parent still has a child session is
  irrelevant — a branch can be built on after its session is gone.

## Edge cases and constraints

- **Branch not yet on remote.** The PR lookup returns `exists = false`; the row shows no PR chip and
  no in-progress indicator until a session claims the branch. Not an error. Its `remote.exists` is
  `false`, which blocks a *descendant's* spawn and warns that the base is not on origin *(2026-07-26)*.
- **No GitHub credential for the calling operator** *(revised 2026-07-26)* — the lookup reports
  `unavailable = true` with a reason, **not** `exists = false`. Previously the daemon read
  `GITHUB_TOKEN`/`GH_TOKEN` from its own environment and an absent token collapsed to "no PR",
  which is exactly how a live PR stayed invisible.
- **Re-login is required** *(2026-07-26)* — tokens minted before the scope widened carry only
  `read:user`; an insufficiently-scoped token surfaces as *unavailable* with a reason.
- **A stub/demo login is a first-class supported state**, not a degraded one *(2026-07-26)*: rows simply
  show no PR, every RPC succeeds, and no error surface appears anywhere in the screen. The stub
  provider (`packages/tddy-github/src/stub.rs`, `github.stub: true`) stores no token, and that must be
  indistinguishable from a repository with no open PRs.
- **`origin/<branch>` is only as fresh as the last fetch** *(2026-07-26)* — a branch pushed by another
  machine reads as missing until this host fetches. The start-blocked warning is therefore
  conservative: it can delay a spawn, never permit one that would fail. The **Repoint** control it
  enables is not conservative, though — offered against a base that is actually alive, taking it drops
  that parent edge from the plan for good.
- **A dangling `session_id` is left in the changeset**, not scrubbed *(2026-07-26)* — the orphan state is
  derived at render, which keeps `DeleteSession` free of a stack scan and self-heals across hosts.
- **`work_on_selected_branch` skips chain-base resolution** *(2026-07-26)* — correct, since no branch is
  created and nothing is fetched. Only the node link needs the effective branch.
- **Branch resolves to more than one session.** Resolution prefers the active session; ties resolve to
  the most recently updated. (Should not happen for a well-formed stack.)
- **Repoint with no local branch.** Git rebase is skipped (remote-only branch); PR base is still
  re-targeted — mirrors `execute_stack_repoint`'s existing "branch not local; skipping rebase" path.
- **Repoint with no branch at all** *(2026-07-26)* — a plan-only edit (D19). Previously an error
  (`node '<id>' has no branch to repoint`), which rejected exactly the planned-but-never-started node
  the recovery is for.
- **Repointing detaches a real dependency when the predecessor simply has not started** *(2026-07-26)* —
  deliberate and operator-driven. The control names its target, so choosing "Repoint to `origin/master`"
  on a node whose predecessor is merely unstarted is an explicit decision to stop stacking on it, not an
  accident. The alternative — offering Repoint only for provably merged-and-deleted bases — leaves the
  indistinguishable cases (branch deleted without merging, never pushed) with no recovery, which is the
  defect being fixed.
- **A repointed node's plan no longer records its original parents** *(2026-07-26)* — dropping them is
  the persisted effect (D18), so the stack's DAG loses that edge. This is the intended meaning of
  "repoint": the node is no longer stacked on that predecessor.
- **Polling churn.** The poll interval is fixed (5 s) and shared per screen; only the branches currently
  rendered — and their base branches, which are themselves rendered nodes' branches — are queried. Since
  2026-07-26 that is **one** GitHub lookup per branch per tick, not two.
- **Multi-parent DAG base.** A node with more than one non-merged parent uses the nearest
  ancestor ref (`effective_base_refs`' first entry) as its single base; a true octopus/merge base
  across multiple parents is out of scope for this changeset (documented non-goal).
- **Out-of-order start.** Starting a node whose non-merged parent owns no branch yet is refused
  (D6). A node whose parents are all merged is a root for base purposes and starts off the stack
  default.
- **Parent's child session closed or cleaned up.** Not an error and not a block: the parent's
  branch is what the child worktree bases onto, and it outlives the session that created it.
- **Parent's branch recorded only by its child session.** The node's `branch` resolves through
  that session's changeset (`resolve_stack_node_branch`), so the descendant still spawns. A missing
  session directory resolves to no branch, which is a refusal, not a crash.

## Panel UX: expandable rows, session links, persisted order, base sync *(added 2026-08-01)*

Everything above made a planned PR's *state* legible. This makes the panel usable as a control surface:
the row collapses to a summary and expands to its full detail, the spawned indicator opens the session
it is bound to, row order is persisted and operator-editable, each row states how its branch stands
against its base, and a row that is cleanly behind its base can pull the base in.

Related: [Session drawer § Planned-PR list](../web/session-drawer.md#planned-pr-list) for the row
anatomy, badges and controls as the operator meets them.

### Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D21 *(2026-08-01)* | A row's detail is **hidden, not unmounted** | Expansion, scroll position and the branch poll set all survive a collapse — the stance `PlannedPrPanel` already takes. It also keeps D16 intact rather than reversing it: the row still renders its full information, one interaction away. Concretely, eight existing acceptance specs assert the detail lines with `exist` / `contain.text`, which pass inside a `display:none` subtree and would all break on an unmount. |
| D22 *(2026-08-01)* | Blockers, refusals and errors stay **outside** the collapse boundary | These are the states that explain why an action is unavailable. A reason the operator has to expand a row to find is the dead end D16 removed. |
| D23 *(2026-08-01)* — **extended by D39** | The bound session is resolved from the **plan first**, the branch's current owner second, and both are guarded on the session being one the drawer knows | The indicator is the *node's* recorded binding and the plan is the durable record; "who owns this branch right now" is a different question whose answer changes after a resume or a hand-off. Without the known-session guard the control would select nothing for a child deleted on another host. |
| D24 *(2026-08-01)* | Display order is **persisted per node**, not derived from `parents` | The dependency graph and the reading order are different facts. Deriving one from the other means every merge, repoint and re-parenting silently rewrites the operator's view — a row they are reading jumps position because of an unrelated event. |
| D25 *(2026-08-01)* | A plan carrying only *some* positions falls back to topological order **wholesale** | A half-numbered plan has no coherent total order, and interleaving real positions with invented ones can render a child above its parent — a worse lie than one render of a correct derived order. The next write numbers every node, after which the fallback stops applying. |
| D26 *(2026-08-01)* | Base sync is computed with a **non-mutating** probe, and never fetches | It runs on the 5 s poll against worktrees that may have a live agent in them, which disqualifies the existing `pr_resolve_conflicts_action` (`git merge --no-commit` then `--abort` — it mutates the index). Fetching is disqualified on cost: the authoritative default-branch resolver runs `git fetch origin`. |
| D27 *(2026-08-01)* | An unavailable comparison is **never** rendered as clean | A failed comparison arrives with no commits behind and no conflicts — byte-identical to a healthy branch. This is [D12](#authenticated-pr-status-added-2026-07-26) restated one level down, and it exists for the same reason: conflating "could not tell" with "nothing to report" is how a live open PR stayed invisible for a day. |
| D28 *(2026-08-01)* | The **base actually compared is reported back**, and is what the row names | The counts are meaningless without the ref they came from, and the base may resolve locally rather than remotely. The row states what was compared, not what it asked for — the same discipline as [D18](#repointing-a-dead-end-planned-pr-added-2026-07-26). |
| D29 *(2026-08-01)* | An **unnamed base is unavailable**, not substituted | `RepointPlannedPr` substitutes the resolved default for an empty target (D20) because it is a mutation the operator asked for. This is a display: the number beside a row must describe the same base the row's own base line shows, and substituting a different one answers a question the row is not asking. The authoritative resolver also fetches. |
| D30 *(2026-08-01)* | Pull **defaults to merge**; rebase is offered beside it | Merge preserves the open PR's review anchors and needs no force-push — it is what the stack's own `pr_resolve_conflicts` already does. Rebase rewrites history and force-pushes, which is the right trade only when the operator picks it. |
| D31 *(2026-08-01)* | A **dirty worktree prompts** to commit and push first | Refusing outright leaves a permanently dead button in any worktree an agent is working in; auto-stashing can conflict on the pop and would leave a child session's checkout in a state nobody asked for. Committing is the one resolution that is explicit, recoverable through git, and leaves the child's work safe. |
| D32 *(2026-08-01)* | A pull **pushes**; a failed push is reported, not rolled back | Without the push the GitHub PR still reports itself behind and the row contradicts the reviewer's view. That the local merge or rebase landed is a fact worth reporting truthfully — undoing it would be strictly worse. |
| D33 *(2026-08-01)* | Conflicts **abort**, and the node is not stamped `has-conflicts` | Leaving `MERGE_HEAD` and conflict markers in a worktree an agent may be mid-turn in would corrupt that turn. (The agent-facing `pr_resolve_conflicts` deliberately leaves them, because it runs *inside* the child's own workflow where the agent is about to be prompted to resolve them.) And with conflicts now a live fact on every poll, a persisted stamp can only go stale — clearing it later would risk stomping an agent's own `source: "override"`. |

### Persisted display order

`StackNode.display_order: Option<u32>` (additive, omitted when unset). `Stack::display_order()` sits beside
`topo_order` and sorts on `(display_order.unwrap_or(u32::MAX), topo_index, node_id)`. It **never errors** —
it is on a render path, so a cycle degrades `topo_index` to declaration order rather than failing, and no
node is ever dropped.

Assignment is on write, not on read: `pr_stack::assign_missing_display_order` is the first statement of
every mutator's `update_stack_atomic` closure, so a legacy stack is numbered the next time anything writes
it. Backfilling inside `read_changeset` was rejected — it would make the value returned differ from the
bytes on disk, breaking the roundtrip contract `pr_stack_data_model_acceptance.rs` pins, and would rewrite
meaning on a path with no write.

- `planned_prs_into_stack_nodes` numbers from the plan's **array index** — the order the agent emitted is
  the intended reading order, and it may legitimately differ from the DAG.
- `add_planned_pr_node` / `adopt_pr_as_stack_node` take `max + 1`; neither renumbers the rows above.
- `delete_planned_pr_node` leaves survivors alone. Gaps are harmless in a sort key, and renumbering would
  be a write across every surviving node purely to make the numbers tidy.
- `reseed_stack_from_plan_if_unspawned` takes the new plan's order wholesale — it already refuses once any
  node owns a branch or a session, so nothing has spawned and no operator has been working against those rows.
- `pr_stack::move_planned_pr_node(session_dir, node_id, "up" | "down")` swaps a node with its neighbour.
  Past either end is a **no-op success**; an unknown node id is refused without writing.

### Base sync

`tddy_core::base_sync` — a new module rather than an addition to `worktree.rs` (already ~2300 lines) and
deliberately not in the recipe crate, since a display probe is not a recipe concern:

- `resolve_base_sync_refs` (cheap: resolve both refs) and `compare_base_sync_refs` (expensive: the counts
  and the conflict probe) are split so the daemon can cache on the two commit SHAs — the comparison is a
  pure function of them.
- A leading `<remote>/` is **stripped off the requested base before probing**. The caller sends
  `ProjectEntry.main_branch_ref`, which is usually already `origin/master`, and
  `refs/remotes/origin/origin/master` resolves to nothing.
- Base resolves remote-first, head local-first; both resolved names are reported (D28).
- `git rev-list --left-right --count <base>...<head>` gives behind and ahead in one process.
  **`behind_count == 0` short-circuits the conflict probe** — nothing to merge in, nothing to conflict.
- Otherwise `git merge-tree --write-tree --name-only -z`: exit 0 clean, exit 1 → the conflicted paths,
  anything else → an error. It writes a tree into `.git/objects` but touches no index, working tree, `HEAD`
  or ref, which is exactly what makes it safe against a live checkout; repeated identical probes produce
  identical objects, which are ordinary gc candidates.
- Every failure is a `Result::Err`, never a zeroed success (D27).

**Cost, and the four bounds on it.** `QueryBranch` was ≈4 git execs per branch per tick. The new leg adds 4
on a cache miss and 2 on a hit, and skips `merge-tree` entirely when nothing is behind. Uncached, a ten-node
stack on the 5 s cadence would roughly double git load, and `merge-tree` is the outlier — proportional to the
merge diff, so hundreds of milliseconds on a large repo with wide base drift. Bounded by: the content-keyed
`base_sync_cache` (keyed `(repo_root, base_sha, head_sha)`, **no TTL** — an entry cannot go stale, it only
becomes unreachable when a ref moves; errors are cached too so a repo that cannot answer is not re-probed
twelve times a minute; capped with oldest-insert eviction), the `behind == 0` short-circuit,
`spawn_blocking_with_timeout`, and skipping the leg when no base was named.

**Freshness.** The probe never fetches, so the counts are only as fresh as the last fetch — the same
conservatism `remote_branch_ref_sha` already ships with (D9). The pull action fetches, and the next tick sees
the truth.

**Worktree dirtiness rides the `worktree` leg, not the sync leg**, and stays **outside** the cache: it is a
property of the checkout, not of the two commits. `git status --porcelain --untracked-files=no` — untracked
files are deliberately ignored, since git refuses loudly rather than clobbering one, while blocking on them
would make the pull control permanently dead in any real agent worktree.

### Pulling the base into a branch

`pr_stack::pull_base_into_node_branch` operates in the node's **own worktree**. The ordering is the safety
design:

1. node absent, node owns no `branch`, or no base named → refused.
2. no worktree for the branch → refused, naming the branch. It deliberately does **not** fall back to
   checking the branch out in the main repo; `git_ops::rebase_onto` does that, and that latent clobbering
   hazard is not extended to a new caller.
3. **worktree cleanliness is checked before the fetch and before anything touches git state.** Dirty refuses,
   unless the caller opted into committing (D31), in which case the tracked changes are committed and pushed
   first.
4. fetch — **base-ref-scoped, not `git fetch origin`**, so a rebase's `--force-with-lease` still sees the
   remote as it was.
5. apply; a conflict is aborted and reported with its paths (D33); already-up-to-date changes nothing.
6. push — plain for a merge, `--force-with-lease` for a rebase. A failed push is a **successful call
   reporting `pushed = false`** with the reason, not an error (D32).

The gate is a clean tree, **not** session activity: an active session with a clean tree is the normal case,
and `is_active` is an unreliable proxy either way. The residual race — the agent writes between the check and
the merge — is bounded by git itself, which refuses a merge that would overwrite local modifications, so the
worst case is a clean refusal rather than lost work.

### API surface *(added 2026-08-01)*

Proto (`packages/tddy-service/proto/connection.proto`):

- `QueryBranchRequest.base_branch = 4` — the base to compare against. Empty means the caller could not name
  one, and the leg reports itself unavailable rather than substituting a default (D29).
- `BranchResolution.base_sync = 6` → `BranchBaseSync { base_branch, behind_count, ahead_count, has_conflicts,
  conflicted_paths, unavailable, unavailable_reason, base_ref, head_ref }`. `base_branch` carries the ref
  actually compared (D28).
- `BranchWorktree.dirty = 3`, `dirty_paths = 4`.
- `rpc ReorderPlannedPr` — `{session_id, node_id, direction}` → `{stack_plan_json}`, the same wire shape
  `AddPlannedPr` and `RepointPlannedPr` already return, so the web reuses `parseStackPlan`.
- `rpc PullBaseIntoBranch` — `{session_id, node_id, base_branch, strategy, dirty_worktree_action,
  commit_message}` → `{resolution, strategy, changed, pushed, push_error}`. It returns a **fresh
  `BranchResolution`** so the row repaints without waiting for the next poll tick.

Both handlers carry the standard prologue ending in `require_pr_stack_orchestrator`. The base-sync leg is
**non-erroring like every other `QueryBranch` leg** — an absent base, an unknown repo root, a probe failure
or a `spawn_blocking` timeout all degrade that leg alone and the RPC still succeeds.

Web (`tddy-web`), all under `src/components/sessions/prstack/`:

- `orderStackNodes` — persisted order when every node has one, else `topoSortStackNodes` wholesale (D25).
  `topoSortStackNodes` is unchanged and remains the legacy fallback.
- `parentTitles` — parent ids to titles, skipping dangling ids.
- `boundChildSession` — the session the indicator opens (D23).
- `branchQueries` — `buildBranchQueries(nodes, defaultBranch)` pairs each owned branch with its base. This
  also closes a hole: a **root** node's base is the project default, which the old poll set never queried at
  all. Every ancestor base is itself some node's branch, so the branch set is unchanged and the GitHub
  request rate does not move.
- `baseSyncStatus` — `baseSyncView` returning `unknown | unavailable | conflicts | behind | in-sync` in that
  precedence, and `canPullFromBase`, which is true only for `behind`.
- `useQueryBranch` takes branch+base queries and additionally returns `setResolution`, so a completed pull
  writes its fresh resolution straight into the hook's map — the map *is* the state, so the write is
  naturally superseded by the next poll and there is no override layer to go stale.
- `DirtyWorktreeDialog` — the commit-and-push-first prompt (D31).

`in-sync` renders a real badge rather than nothing: if only the bad states rendered, a healthy row and a row
whose poll has not answered would look identical — D27 one level down.

## Cross-host planned PRs, a PR without a session, and force start *(added 2026-08-30)*

Everything above assumes the orchestrator, its children, their worktrees and their branches live on
**one** daemon. They routinely do not: the Start-session dialog lets the operator pick the host, and
the drawer already shows sessions from every host in the fleet. Starting a planned PR's session on a
different host than its orchestrator made the PR-Stack view lose the node entirely — the row fell back
to offering **Start session** for work that was already running.

Three separate mechanisms produce that one symptom.

**The node link is written on the wrong host.** `link_stack_node_to_spawned_branch`
(`connection_service.rs`) records `session_id` + `branch` on the planned node by writing the
orchestrator's `changeset.yaml` **on the spawning daemon's own sessions tree**. When the child spawns
on host B and the orchestrator lives on host A, host B has no such session, the lookup returns
`None`, and the write is silently skipped. The node stays branchless and childless forever; its
descendants stay unspawnable, because `Stack::base_ref_for_spawn` gates on a parent owning a branch.
The code has carried a `TODO(cross-host-pr-stack)` naming this exact gap.

**Three of `QueryBranch`'s six legs are local-only false negatives.** The `session` leg is a
`read_dir` over this daemon's sessions directory (`branch_owner::find_session_owning_branch`); the
`worktree` leg is `git worktree list` in this daemon's checkout; the `remote` leg is
`git rev-parse refs/remotes/origin/<branch>` against this daemon's remote-tracking refs. Each answers
`exists = false` for a branch that is alive one host over, and — unlike the `pr` and `base_sync` legs
— none carries an `unavailable` discriminator, so "cannot see it from here" is indistinguishable from
"does not exist". The `pr` leg is the exception: it asks the GitHub API by head branch, so it is
already host-independent.

**A claude-cli session publishes no participant metadata at all.** The drawer's cross-host rows come
from LiveKit presence in the common room, and a participant's `session` metadata block is what
hydrates them (`crossHostSessions::mergeActiveAndFetchedSessions`). Only `tddy-coder` publishes that
block; a claude-cli session's LiveKit bridge (`cli_session_manager::spawn_livekit_bridge`) calls
`participant.run()` with no metadata watch. Planned-PR children are claude-cli sessions, so their
synthesized rows carry **no `branch` and no `orchestrator_session_id`** — the two keys every PR-stack
join uses (`resolveNodeSession`, `boundChildSession`, `sessionStackGroups`).

Alongside this, two adjacent gaps the same operator hits:

- A planned PR that owns no `branch` is never polled at all (`buildBranchQueries` skips it), so a PR
  opened for its planned branch — by a session on another host, or by a session that has since ended
  — is invisible in the row that exists to track it.
- Every blocker **disables** Start session (D16). Two of the three blocker kinds are now known to be
  derivable from the false negatives above, so the one control that would recover the node is the one
  the mis-derivation takes away.

### Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D34 *(2026-08-30)* | A child spawn carries the **planned node's id** — `StartSessionRequest.stack_node_id` — rather than the daemon re-deriving the node from the branch the spawn creates | `pr_stack_node_for_spawn` matches on `new_branch_name`, which the operator can edit in the create dialog before confirming; a rename silently unlinks the node from the row that started it. And on the cross-host path the spawning daemon cannot read the orchestrator's stack *at all*, so there is nothing to derive from. The node id is the one fact the surface that rendered the button already knows exactly. Empty stays supported: the agent's own `spawn-child` runs on the orchestrator's host, where the branch-derived lookup is correct and local. |
| D35 *(2026-08-30)* | The node link is written by a **peer-routed RPC** — `LinkStackNode`, routed through `rpc_served_by_peer` exactly as `ResolveStackBase` is — not by the spawning daemon's local disk write | This is the write half of the read `ResolveStackBase` already performs, and the same argument applies verbatim: the parent's stack lives in *its* session's `changeset.yaml`, so only the daemon holding that session can write it. Routing before authentication, as the other peer RPCs do: a token is verified by the daemon that serves the call. |
| D36 *(2026-08-30)* | A failed link **does not fail the spawn** — it is logged, and the live association still travels in participant metadata (D37). This now covers the **local** write too: `link_stack_node_to_spawned_branch` used to be called with `?`, so an unwritable orchestrator changeset on this host failed the start | The failure lands *after* the worktree, the branch and the session exist. Failing the spawn there would leave an orphan session on host B and still no branch on host A, which is strictly worse than a node the operator can re-link by restarting it. The argument does not turn on the host: a local disk failure leaves exactly the same orphan, and the spawn that raised it is exactly as complete. Keeping the local write fatal while the routed one is not would also make the same operator action succeed or fail on where the orchestrator happens to live. The daemon's own spawn gate is unaffected: a descendant of an unlinked node is refused for the real reason. |
| D37 *(2026-08-30)* | The session's LiveKit participant publishes its **stack association** — `session_id`, `orchestrator_session_id`, `stack_node_id`, `branch` — in the `session` metadata block, and the daemon publishes that block for **claude-cli sessions**, which previously published none | Presence is the only cross-host signal the web has: `ListSessions` does not fan out (only `ListProjects` does), and adding a fan-out would put a per-host RPC on a 2 s poll to answer a question presence already answers for free. The shape moves to `tddy_core` so the coder's publisher and the daemon's cannot drift — the merge into participant metadata is **shallow**, so a partial `session` object would erase the other publisher's keys rather than merge with them. |
| D38 *(2026-08-30)* | The view joins on a **domain type of its own** — `StackChildSession { sessionId, orchestratorSessionId, stackNodeId, branch, isActive }` — assembled from the session list plus the parsed metadata the drawer already keeps (`sessionMetadataBySessionId`). **No new `SessionEntry` field.** `mergeActiveAndFetchedSessions` hydrates `branch` and `orchestrator_session_id`, which the row shape already has | The two facts the synthesized row was missing are existing proto fields; only `stack_node_id` is new, and it is needed exactly where a participant is live — which is exactly where the metadata map has an entry. A same-host child with no participant needs none: the node's own link was written correctly on its own host. Keeping the join off the proto row also stops the PR-stack join from being coupled to the `ListSessions` shape, which is what made it untestable without a protobuf message. |
| D39 *(2026-08-30)* | A node's live child resolves **by `stack_node_id` first**, then the plan's recorded `session_id`, then branch ownership | Extends D23's ordering rather than replacing it. The node id is an exact identity that survives a branch rename and a host boundary; the plan's record is the durable second-best; "who owns this branch right now" stays last, as D23 already argued. |
| D40 *(2026-08-30)* — **amends D7** | A live participant claiming the node **suppresses the orphan verdict** | D7 called `QueryBranch` the authority because it "scans sessions by their changeset branch" — true, and it scans exactly **one host's**. `session.exists = false` therefore means "not on the daemon I asked", which is precisely the cross-host case, so on its own it would render every remote child as a deleted one and offer a duplicate spawn for a session that is mid-turn. Presence is the one signal that *positively* proves the session exists; absence of presence still falls through to D7 unchanged. |
| D41 *(2026-08-30)* | A node that owns **no branch** is additionally polled on its `branch_suggestion`, and **only the `pr` leg of that resolution is read** | The `pr` leg is the one leg of `QueryBranch` that is host-independent, so it is the one leg that can answer for a branch this daemon cannot see. Reading any other leg off a suggestion would violate D1: a suggestion is a planned name, not a ref, and letting it feed base resolution or the spawn gate would unblock a spawn onto a ref nothing created — the exact failure D1 exists to prevent. The row therefore gains a PR link and state; its base line, its blockers and its `branch` line are untouched. |
| D42 *(2026-08-30)* — **amends D16** | **Start session is never disabled.** A blocked node renders it in a warning colour with an alert icon and a tooltip naming every blocker; the row keeps its warning box (D22) | D16 established that a blocked row keeps its information and its button, and that Repoint is what re-enables the button. That reasoning assumed the blockers were *true*. Two of the three are now known to be derivable from local-only false negatives — `remote.exists` reads absent for any branch pushed from another host until this clone fetches, and `parent-has-no-branch` is true for every parent whose link was written on the wrong host — so the gate refuses spawns that would in fact succeed, and no repoint fixes them. A gate that cannot see half the fleet must **advise, not refuse**. The daemon still enforces its own gate: a genuinely impossible spawn fails there, with the real reason, which is strictly more informative than a button that cannot be pressed. |
| D43 *(2026-08-30)* | Force start needs **no extra confirmation step** | `CreateSessionDialog` already stands between the click and the spawn, showing the base branch, the branch name and the prompt the child will be started with. A confirm dialog in front of a review dialog is a click, not a safeguard. |

### `LinkStackNode`

Mirrors `ResolveStackBase` in shape, routing and refusal semantics:

```proto
rpc LinkStackNode(LinkStackNodeRequest) returns (LinkStackNodeResponse);

message LinkStackNodeRequest {
  string session_token = 1;
  // The daemon whose sessions tree holds `orchestrator_session_id`. Empty or matching the local
  // instance = this daemon answers from its own disk.
  string daemon_instance_id = 2;
  // The pr-stack orchestrator whose plan holds the node.
  string orchestrator_session_id = 3;
  // The planned node to link. Required — a link is never derived from the branch on this path (D34).
  string node_id = 4;
  // The child session that materialized the node.
  string child_session_id = 5;
  // The branch that session created. Recorded as the node's `branch`, which is what makes the
  // node's descendants spawnable.
  string branch = 6;
}
message LinkStackNodeResponse {
  // The plan as it stands after the link, so a caller that renders it does not re-read.
  string stack_plan_json = 1;
}
```

- Routed **before** authentication (`rpc_served_by_peer`), like every other peer-routed RPC.
- `require_pr_stack_orchestrator` on the resolved session; an unknown `node_id` is
  `not_found`, never a silent success — a link that quietly does nothing is what the current local
  write already does, and is the bug.
- The write itself is the existing `link_stack_node_to_child_session`, unchanged.
- The spawn path calls it in place of `link_stack_node_to_spawned_branch`'s local write whenever the
  spawn carries a `stack_node_id`; a spawn without one keeps the branch-derived local lookup, so the
  agent's own `spawn-child` (always on the orchestrator's host) is untouched.

### The `session` metadata block

The shape moves to `tddy_core::session_participant_metadata` and gains four keys, all additive and
all tolerated as absent by older participants:

```json
{ "session": { "workflow_goal": "", "workflow_state": "", "agent": "", "model": "",
               "activity_status": "", "recipe": "", "repo_path": "", "elapsed_display": "",
               "pending_elicitation": false,
               "session_id": "", "orchestrator_session_id": "", "stack_node_id": "", "branch": "" } }
```

`tddy-coder` publishes it as today, seeded with the new fields from its CLI args
(`--stack-parent`, the new `--stack-node-id`) and its changeset's `branch`. The daemon publishes it
for a claude-cli session by handing `spawn_livekit_bridge` a metadata watch — carrying the **static**
identity and association fields the daemon knows at spawn (`session_id`, `orchestrator_session_id`,
`stack_node_id`, `branch`, `agent`, `recipe`, `model`, `repo_path`). The live workflow fields stay
empty for such a session, exactly as they are today; nothing that renders is lost, and the cross-host
drawer row gains a real name where it previously had only a short session id.

The block is **re-published on a 30 s timer** rather than sent once. The metadata watcher logs and
drops a failed `set_metadata`, and a room rejoin starts the participant's wire metadata over, so a
publisher that sends one value and never another loses the association permanently on either —
the exact symptom this section exists to remove. `tddy-coder` needs no timer because every workflow
transition re-sends on the same channel; a claude-cli session has no workflow tap, so the timer is
its only other occasion to publish. It is the shape the participant's other publishers (the
codex-OAuth and owned-project-count pollers) already use, for the same reason.

### What is deliberately *not* fixed here

- **The `worktree` and `remote` legs stay local-only.** Making them host-aware means routing
  `QueryBranch` per branch to whichever daemon owns the checkout, which is a per-branch fan-out on a
  5 s poll. Force start (D42) is the cheaper answer to the operator-visible half; the residual is that
  a cross-host node's row shows no worktree path and no remote sha. Logged in `docs/dev/TODO.md`.
- **`resolveRepointTarget` still skips a parent whose branch reads absent from this host's origin**,
  so a repoint can name the default branch where a parent would have served. The target is stated on
  the button before the click (D18), so the operator sees it; nothing is done behind their back.
