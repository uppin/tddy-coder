# 2026-07-26 — pr-stack-ux-recovery

**Type:** Fix (four independent defects) · **Branch:** `feat-fix-pr-stack`
**Packages:** `tddy-service`, `tddy-core`, `tddy-github`, `tddy-workflow-recipes`, `tddy-daemon`, `tddy-web`
**Index line:** [docs/dev/changesets.md](../changesets.md)
**Features:** [pr-stack-live-status.md](../../ft/coder/pr-stack-live-status.md) ·
[pr-stacking.md](../../ft/coder/pr-stacking.md) ·
[session-drawer.md § PR-Stack Chat Screen](../../ft/web/session-drawer.md#pr-stack-chat-screen) ·
[session-auth.md § GitHub access-token retention](../../ft/daemon/session-auth.md#github-access-token-retention-added-2026-07-26)
**Per-package:** [tddy-service](../../../packages/tddy-service/docs/changesets.md) ·
[tddy-core](../../../packages/tddy-core/docs/changesets.md) ·
[tddy-workflow-recipes](../../../packages/tddy-workflow-recipes/docs/changesets.md) ·
[tddy-daemon](../../../packages/tddy-daemon/docs/changesets.md) ·
[tddy-web](../../../packages/tddy-web/docs/changesets.md)
(`tddy-github` has no `docs/` directory; its changes are recorded here and in the daemon/feature docs.)

## What was broken

Four operator-facing defects made the PR-Stack Chat Screen unusable once a stack had been partly worked.
All four were reproduced against a live stack (orchestrator session
`019f9dd5-716d-7071-96ac-464ff7b98c2a`, project `uppin/tddy-coder`).

1. **Deleting a child session wedged its planned PR permanently.** `DeleteSession` removes a session
   directory and never touches any orchestrator's `Changeset.stack`, so the node kept the deleted
   session's id — verified live, where node `attach-store` still recorded
   `session_id: 019f9e02-…` for a directory that no longer existed. `PlannedPrRow` derived its mode from
   `isSpawned = Boolean(node.sessionId)`, so the row showed a status chip forever and the Start-session
   button was never reachable again. Re-enabling the button alone would not have fixed it: the node
   already owned a pushed branch with a worktree on disk, and the screen pre-filled
   `new_branch_from_base` with that same name, so the spawn would fail on "branch already exists".
2. **A node whose base branch was absent from `origin` still offered "Start session".** The spawn gate
   was pure changeset metadata (`Stack::base_ref_for_spawn` refuses only a non-merged branchless
   parent); no git-level check happened at all. `git fetch origin <base>` inside worktree creation
   caught it **after** `StartSession` had been accepted and the session directory and changeset were
   written, leaving a broken session behind. Nothing in the row said the node was not startable.
   `Changeset.remote_pushed` was write-only, and the poll set contained only branches that already
   existed — so the base, the thing that determines startability, was resolved only incidentally.
3. **A planned PR with a live GitHub PR showed neither its branch nor its PR number.** Three independent
   causes: the branch name was never rendered by any code path; the daemon held no GitHub token
   (`github_token_from_env` reads only `GITHUB_TOKEN`/`GH_TOKEN`, unset under systemd) and
   `get_pr_by_head` returned `Ok(None)`, indistinguishable from "no PR exists"; and `head` was passed
   unqualified, which GitHub **ignores** rather than rejects. PR #351 on
   `feature/session-attach-docs/attach-proto` was open and never appeared. The token the operator had
   already granted was discarded at the end of `exchange_code`, and the authorize scope was `read:user`
   only, which cannot read PRs on a private repo.
4. **The planned-PR list was a fixed half-width pane** (`w-1/2`, no toggle, no breakpoint handling): it
   permanently halved the chat on desktop, was unusable on mobile, and could not be dismissed.

A fifth problem surfaced in review: `GetPrStatus` and `QueryBranch` funnel into the same authenticated
`GET /pulls`, and the screen polled **both** — 2 × 5s × branches ≈ 1440 requests/hour/branch against a
5000/hour user limit, exhausted within the hour on a five-node stack, after which every row read "PR
status unavailable" permanently.

## What was decided

The full decision table (D1–D15) lives in
[pr-stack-live-status.md § Design decisions](../../ft/coder/pr-stack-live-status.md#design-decisions).
The load-bearing ones:

- **Orphan state is derived at render, from a server fact.** A node is orphaned when it records a
  `session_id` *and* its `QueryBranch` resolution has **arrived** with `session.exists = false`.
  Deriving it from the web's `sessions` list would misread a node as orphaned whenever its host was
  merely offline; requiring arrival keeps the first render deterministic. The dangling `session_id` is
  deliberately **not** scrubbed — that keeps `DeleteSession` free of a stack scan and self-heals across
  hosts, at the cost of a stored stack that does not match reality (recorded as a follow-up).
- **Resuming, not recreating.** An orphaned node that owns a branch is restarted into
  `work_on_selected_branch` on that branch. This required the daemon to key its node link on the spawn's
  **effective branch** — otherwise `pr_stack_node_for_spawn` returns `None` for the blank
  `new_branch_name` a resume sends, the recovery never sticks, and every click spawns another unlinked
  session. `resolve_chain_base_ref` deliberately keeps keying on `new_branch_name`: a resolved chain
  base makes worktree setup run a real fetch, which can fail and is pointless for a resume.
- **Startability is a server-resolved fact about `origin`.** `BranchResolution.remote` carries it.
  Deriving it in the web from `remote_pushed` or from a non-empty branch name would report *available*
  for a local-only branch — precisely the case that fails inside `git fetch`.
- **"Missing branch" replaces the button** rather than disabling it. A disabled control with no
  explanation is the dead end operators already hit; the indicator names the branch being waited on, so
  the next action (start the predecessor) is obvious.
- **The operator's own GitHub token, retained server-side.** Chosen over an ambient `gh auth token`
  fallback (invisible state) and over embedding the token in the HMAC session token (which would put a
  live `repo` token into browser storage on a plain-http origin). The token never leaves the server.
- **Unavailable ≠ no PR, and a stub login is neither.** `PrStatusView.unavailable` +
  `unavailable_reason` exist because the silent `Ok(None)` is the whole reason a live PR was invisible
  for a day. A stub/demo login (`github.stub: true`) short-circuits to a clean empty result and must
  stay indistinguishable from a repo with no open PRs — a demo must never show an error banner. This is
  enforced through `GitHubOAuthProvider::issues_usable_access_token()` with **no default impl**, so
  every provider states its answer; that is how it is enforced without a test-environment branch.
- **A login that cannot retain its token fails.** A session minted without its token is a half-login:
  apparently signed in, every GitHub read *unavailable*, and re-authenticating is the one remedy the
  operator has no reason to attempt. Correspondingly, a configured-but-unwritable `auth_storage` fails
  at **boot**, not per login.
- **The panel copies the Session Inspector's contract** — always mounted, `data-state` drives
  visibility — so there is one overlay idiom in the codebase. It is mounted inside `PrStackScreen`, not
  `SessionMainPane`, to keep pr-stack state out of the shared pane.

Two deliberate deviations from the written plan: `relative` sits on the screen's **content row** rather
than the screen root, so the absolute panel never covers the header toggle that opens it; and width and
closed-visibility are **inline styles driven by `isMobile`** rather than `w-full md:w-[360px]` + the
Tailwind `hidden` class, because component tests mount without the app stylesheet, so a
media-query/class-only panel would have no width and no way to hide (the idiom `SessionDrawer` already
established for the same reason).

## Verification

- `tddy-github` 30/30.
- `tddy-core` + `tddy-github` + `tddy-daemon`: 1189 passed / 11 failed — all 11 are the pre-existing
  cgroups-sandbox failures on this host (see `docs/dev/TODO.md` and the workspace-failure notes).
- `bun test src/` 587 / 0.
- PR-stack Cypress 91 / 92. The one failure, `PrStackChatSystemMessagesAcceptance`, is **pre-existing**
  and was confirmed by stashing all changes.
- `cargo clippy --all-targets -D warnings` clean; `cargo fmt --check` clean.
- **Not done: manual verification against the live stack** (session
  `019f9dd5-716d-7071-96ac-464ff7b98c2a` — node `attach-store` recovers and PR #351 shows on
  `attach-proto`). It needs the daemon rebuilt and installed, `auth_storage` created, and a re-login.
  Carried forward in `docs/dev/TODO.md`.

## Operator migration

Breaking for running deployments, in two ways — both documented in
[session-auth.md § Operator migration](../../ft/daemon/session-auth.md#operator-migration-breaking-for-running-deployments)
and in the `github:` block of the production config template (`daemon.yaml.production`):

1. The OAuth scope widened from `read:user` to `read:user repo`, so **every already-signed-in operator
   must log out and log in again**. An existing grant cannot be widened in place.
2. A configured `auth_storage` must exist and be writable by the daemon user, or the daemon **refuses to
   start**. `./install` creates and chowns only the parent `/var/lib/tddy`.

## Deliberately out of scope

- Scrubbing dangling `session_id` links from orchestrator changesets on delete.
- Reducing GitHub poll volume further (batching one call per stack, ETags, back-off). The double-poll is
  fixed; the 5s interval itself is untouched.
- Reconciling Repoint availability with the live PR poll (a known limitation carried forward from
  `pr-stack-live-status`).
- Fetching `origin` on demand to refresh remote-branch state — the indicator is only as fresh as the
  last fetch, and is conservative by design.
- Wiring `start_sandboxed_cursor_cli_session` (takes no `stack_parent` at all) and the
  `tddy-coder`-embedded web server (builds its own `AuthServiceImpl` with no token store).
