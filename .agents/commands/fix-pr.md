---
description: Fix a PR - address review comments AND failing checks; signal verdicts with GitHub reactions (agree = thumbs-up, disagree = thumbs-down + reply, fix pushed = rocket), rebase a stacked branch first, and while checks are still running poll and fix failures as they appear instead of waiting for the full run
---
## Fix PR — address review comments and failing checks

Work a PR toward mergeable, end to end. Two workstreams feed one fix loop:

- **Review comments** — triage each one, **signal the verdict on GitHub itself via reactions**,
  fix what deserves fixing.
- **Checks** — diagnose every failed check; if checks are still running, **poll and start fixing
  failures as they appear** — never idle until the whole run completes.

The reactions are the status protocol reviewers see, so they are not optional decoration:

| Reaction | Meaning | When |
|---|---|---|
| 👍 `+1` | Agree — a fix is coming | Immediately after triage, **before** starting the fix |
| 👎 `-1` + reply | Disagree — reply explains why, no code change | After the user approves the drafted reply |
| 🚀 `rocket` | The fix for this comment is pushed | After the push succeeds, on each addressed comment |

A comment can accumulate 👍 then 🚀 (agreed, then fixed). A 👍 is a promise — never 👍 a comment and
then silently drop the fix; if implementation proves the comment wrong after all, reply saying so.

## Step 0: Resolve the PR

- Default: the PR of the **current branch**. `$ARGUMENTS` may name a PR number or branch instead.
- ```bash
  repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)   # uppin/tddy-coder
  gh pr view <N> --json number,title,url,state,isDraft,baseRefName,headRefName,reviewDecision
  ```
- If the PR is merged or closed, stop — there is nothing to fix; report and suggest
  `/follow-up-branch` if the user wants to act on post-merge comments.
- If the working tree has uncommitted tracked changes, stop and ask — PR fixes must not be
  entangled with unrelated in-flight work.

## Step 1: Stack currency — BEFORE reading any diff or writing any fix

Determine whether this branch is part of a PR stack, and **which of the two kinds of stack** it is —
the rest of this command says which one it means at each step:

- **Planned stack** — a `pr-stack` orchestrator session owns the DAG in its changeset
  (`Changeset.stack`, a graph of `StackNode`; see
  [`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md)). The branch you are on was
  spawned from that orchestrator and the node's per-PR documents (`PRD.md`, `changeset.md`) are
  attached to **this** session. The orchestrator agent — not you, in the child session — holds the
  `pr_*` tools; `pr_stack_status` is what reports the node's parents and its effective base.
- **Ad-hoc chain** — someone opened this PR on top of another open PR's branch, with no
  orchestrator. Detect it the way `.agents/commands/pr.md` already does:
  ```bash
  gh pr list --state open --json number,headRefName,baseRefName
  # for each other open PR's head, test ancestry against HEAD:
  git fetch origin <headRefName> && git merge-base --is-ancestor origin/<headRefName> HEAD
  ```
  Any `baseRefName` that is not `master`/`main` is a stack parent.

Then:

- **Stack member (either kind)** → run `/pr-stack-rebase` in **single mode** on this branch now.
  This is a hard gate exactly like `/green`'s: comments and check failures are evaluated against the
  code, and a stale merge-base makes leaked ancestor commits look like this PR's work.
  Already-current is verify-and-return — cheap, never a skip. Never widen into cascade mode from
  here.
- **Ordinary branch** → make sure the branch is current enough that comments map onto the code
  (`/merge` if the user wants `master` merged in; do not do it unprompted).

If the rebase force-pushed, note it in the report — the push restarts CI (fold that run into the
Step 3 loop), and review comments on rewritten lines become "outdated" on GitHub, but they still
get triaged (Step 2).

## Step 2: Collect workstream A — review comments

Gather **all three** comment surfaces:

1. **Review threads** (inline code comments) — GraphQL gives resolution state, the REST
   `databaseId` needed for reactions, and whether we already reacted:
   ```bash
   gh api graphql -F owner='{owner}' -F name='{repo}' -F pr=<N> -f query='
     query($owner:String!,$name:String!,$pr:Int!){
       repository(owner:$owner,name:$name){ pullRequest(number:$pr){
         reviewThreads(first:100){ nodes{
           isResolved isOutdated path line
           comments(first:50){ nodes{
             databaseId url author{login} body
             reactionGroups{ content viewerHasReacted }
           }}
         }}
       }}
     }'
   ```
2. **Review summary bodies** — `gh pr view <N> --json reviews`. These do **not** support
   reactions; a verdict on one is expressed as a reply comment instead.
3. **PR-level conversation comments** — `gh api repos/$repo/issues/<N>/comments` (each has an
   `id` usable with the issue-comment reactions endpoint).

**In a planned stack, the orchestrator can read the same feedback without leaving its chat.**
`pr_comments` returns a PR's submitted reviews, diff-anchored threads and conversation comments, and
`pr_read` returns the PR in full — title, body, state, base/head, mergeability, one latest review
state per reviewer, and the head commit's check runs. Two contract details matter when you use them
instead of `gh`: **no thread is reported as resolved** (thread resolution is GraphQL-only, and the
REST-backed tool refuses to guess), and the tools carry no reaction state — so reactions and the
`viewerHasReacted` idempotency check still go through `gh api` from a shell. In a **child** session
those tools are not available at all; use the `gh` calls above.

Scope rules:

- **Skip resolved threads** and comments the PR author already replied to with a resolution —
  unless the user named them explicitly.
- **Outdated threads still count**: the concern may survive the code that moved. Triage them
  against the current code.
- **Idempotency**: `viewerHasReacted: true` for a given content means this run (or a previous one)
  already posted that reaction — never react twice. A comment already carrying our 🚀 is done;
  re-verify only if the user asks.
- Pure-automation noise (CI status bots, the force-merge trace comment `.github/workflows/automerge.yml`
  leaves behind) is not a review comment — ignore it. A **failing-check report** is workstream B
  input, not a review comment.

## Step 3: Collect workstream B — checks, without waiting for the full run

Every PR gets four required checks, all defined in `.github/workflows/ci.yml`
(see [`docs/dev/guides/ci.md`](../../docs/dev/guides/ci.md)):

| Check | What it runs |
|---|---|
| `Rust lint` | `cargo fmt --all --check`, then `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| `Rust build` | `cargo build --workspace --bins --examples --locked` |
| `Rust tests` | `cargo nextest run --workspace --profile ci --locked` |
| `Web tests` | `bun install --frozen-lockfile`, `bun run build`, `tddy-web` unit tests, `tddy-web` + `tddy-livekit-web` Cypress component tests |

`VM boot control` (`.github/workflows/vm-tests.yml`) also runs on PRs but is **not required** — treat
a red one as a report, not a merge blocker, and say so.

Read them with the repo's own script, which turns a red check into failing **test names** rather
than a colour:

```bash
scripts/ci-status.sh <N>              # per-check state plus "N tests run, M passed, K failed"
scripts/ci-status.sh <N> --failures   # + failing test names, files, assertion messages, failing-step log tails
scripts/ci-status.sh <N> --watch      # block until the run finishes, then report
```

- **Failed checks** enter the fix queue immediately.
- **Pending checks do not block the loop.** Fix what is already actionable (failed checks, agreed
  comments) and re-run `scripts/ci-status.sh <N>` between units of work. New failures join the queue
  **as they appear** — a check that fails at minute 3 gets diagnosed while its siblings still run.
  Only when the queue is empty and checks are still pending does polling become the foreground
  activity, and only then is `--watch` the right call — never open a blocking watch while there is
  still fixable work in the queue.

**Diagnosing a failed check.** The counts and failing test names come from check runs published by
`mikepenz/action-junit-report`, so they are available over the API with no artifact download; the
underlying `gh` calls, and the raw `junit-rust` / `junit-web` artifacts, are listed in
`docs/dev/guides/ci.md` § Reading results. Classify each failure:

| Class | Action |
|---|---|
| **Change-caused** (this PR broke it) | Fix queue — treat like an agreed review comment |
| **Lint / format** (`Rust lint`) | Fix queue. `cargo fmt` is mechanical; a clippy lint is a real finding — fix the code, do not `#[allow]` it without saying why in the PR |
| **Missing fixture binary** | `Rust tests` / `Web tests` exec a few workspace binaries by path (`tddy-sandbox-runner`, `tddy-acp-stub`, `examples/echo_server`) that arrive from `Rust build`'s `rust-fixture-bins` artifact. A test that shells out to a **new** workspace binary passes locally and fails in CI with "not built" — add it to the staging list in `.github/workflows/ci.yml` |
| **Stale lockfile** | Every CI cargo invocation is `--locked`. A dependency change that was not committed as `Cargo.lock` fails all three Rust checks at once |
| **Flaky / infra** | Reported as **flaky**, not silently passed — the `ci` nextest profile retries twice with backoff. The known one is the LiveKit testkit's port TOCTOU (`packages/tddy-livekit-testkit/src/livekit_testkit.rs:26`). Report with evidence; **ask** before re-running a workflow — never re-run, cancel, or force a check unprompted |

Reproduce a change-caused failure locally before fixing — fixing from log text alone is guessing:

```bash
./test -p <package>          # one package
./test -- <test_name>        # one test; output also lands in .verify-result.txt
cargo clippy -- -D warnings
./dev bun run --filter tddy-web cypress:component    # web component specs
```

Scope the local run to the packages your change touched: a full-workspace run carries pre-existing
noise, and reporting that noise as your PR's failure wastes the reviewer's time. Say which scope you
ran. Note that the CI gate deliberately excludes VM-backed tests, cgroups sandbox tests and Cypress
e2e specs (`docs/dev/guides/ci.md` § What the gate does not cover) — if your change touches those,
green CI is not coverage and you have to run them locally (`./vm-tests`, `bun run cypress:e2e`).

## Step 4: Triage comments — one verdict per comment

Read the referenced code and evaluate each comment on its merits. **Challenge, don't defer**: a
reviewer being the author of the comment does not make it correct, and the developer being the PR
author does not make the code correct. Verdicts:

| Verdict | Criteria | Action |
|---|---|---|
| **Agree** | The comment identifies a real defect, risk, or clear improvement in scope for this PR | 👍 now; fix in Step 5 |
| **Disagree** | The comment is factually wrong, or the change would make things worse / violates a repo boundary | Draft a reply with the reasoning; 👎 + post after user approval |
| **Question** | The comment asks for information, not a change | Reply with the answer; no reaction, no fix |
| **Out of scope** | Valid, but belongs to another PR/stack node or a follow-up | Reply saying where it belongs (name the node, or offer to log it in `docs/dev/TODO.md`); no 👍 — that would promise a fix here |

**"Belongs to another node" is a real boundary in a stack, not a dodge.** A planned node's
`## Dependencies` heading lists what a predecessor owns and this PR must **not** implement — that
heading is the repo's duplicate-development guard
([`docs/ft/coder/pr-stack-docs.md`](../../docs/ft/coder/pr-stack-docs.md)). A comment asking you to
implement a symbol another node owns is routed there, not satisfied here. Equally, it is **not** a
licence to answer a comment with a stub: every node must be independently reviewable and
independently mergeable, and a node that ships only surface is not a valid PR — see
[`docs/ft/coder/pr-stacking.md` § PR boundary contract](../../docs/ft/coder/pr-stacking.md#pr-boundary-contract-every-node-is-self-contained).

Post the 👍 reactions as soon as triage lands (this is the "I've seen it, fix coming" signal):

```bash
gh api -X POST repos/$repo/pulls/comments/<databaseId>/reactions -f content='+1'   # review comment
gh api -X POST repos/$repo/issues/comments/<id>/reactions       -f content='+1'   # PR-level comment
```

**Disagreements are gated.** The reply argues with a teammate in public under the user's GitHub
identity — show the user every drafted disagreement (and out-of-scope) reply and get approval
before posting. Then:

```bash
# the approved reply goes into a scratch file — drafted text and anything quoted from the
# reviewer's comment never gets interpolated inline into shell source
gh api -X POST "repos/$repo/pulls/<N>/comments/<databaseId>/replies" -F body=@tmp/reply.md   # into the thread
gh api -X POST "repos/$repo/pulls/comments/<databaseId>/reactions"   -f content='-1'
```

(For a PR-level comment, reply with `gh pr comment <N> --body-file tmp/reply.md` quoting the
comment, and react on the issue-comment endpoint. `tmp/` is gitignored, so a scratch reply cannot
leak into the PR's diff.)

Present the full triage table (comment → verdict → planned action) before moving to fixes.

## Step 5: The fix loop — fix, push, re-check, repeat

Process the queue (agreed comments + change-caused/lint check failures) as one loop:

1. **Fix** the current batch:
   - Fixes only — no drive-by refactors, no scope creep beyond what the comments/failures ask.
   - A comment or failure about behaviour gets a test that pins the fix
     (see [`docs/dev/guides/testing.md`](../../docs/dev/guides/testing.md) and
     `.cursor/rules/testing-practices.mdc`).
   - Never add a fallback to make a failure go away, and never branch on a test environment in
     production code — both are explicit repo prohibitions. Mark anything genuinely temporary with
     `TODO`/`FIXME`. Ask before adding a dependency or deleting a file.
   - `cargo fmt`, `cargo clippy -- -D warnings` and the affected packages' tests
     (`./test -p <package>`) must pass locally before the push.
   - If a fix turns out to be wrong or infeasible mid-implementation, downgrade the verdict:
     reply on the thread explaining what was found (the earlier 👍 must not be left dangling).
2. **Commit and push** per `/update-pr` discipline: only files relevant to the fixes, on the
   current branch, message referencing what was addressed. Never `--no-verify`, never amend.
   Plain `git push` — the only force-push in this command is the one Step 1's rebase already did.
   Batch sensibly: one push per round of fixes, not one per comment — every push cancels the run
   still in flight (`ci.yml` sets `cancel-in-progress: true`) and starts a new one.
3. **Mark the addressed comments** — for each comment whose fix is now on the remote:
   ```bash
   gh api -X POST repos/$repo/pulls/comments/<databaseId>/reactions -f content='rocket'
   ```
   - Optionally (and when the fix is non-obvious, do) reply on the thread with the commit SHA:
     `Fixed in <short-sha>`.
   - **Never resolve review threads** — resolution is the reviewer's acknowledgement, not the
     author's claim. The 🚀 is our half of the handshake.
   - 🚀 goes only on comments whose fix is actually pushed and verified — a partially-addressed
     comment gets a reply describing what remains, not a rocket.
4. **Re-check** — the push restarted CI. Return to Step 3's poll-and-queue rhythm: fix new
   failures as they appear, and exit the loop when every check has completed green (or is
   classified flaky/infra and reported) and every comment has its verdict acted on.

**Convergence guard**: if the same check fails on a **third** consecutive push, stop the loop and
report — repeated red on the same target means the diagnosis is wrong or the failure is
environmental, and more blind pushes only burn CI. Likewise stop and ask if a fix for one failure
keeps causing another.

**This command does not merge.** Landing the PR is `/squash-pr` (single PR) or `/merge-pr-stack` (a
whole stack), and the merge gate is the `#automerge` comment described in
`docs/dev/guides/ci.md` § Automerge. Never post `#automerge` — and above all never `#forcemerge`,
which merges past red or still-running checks — from here.

## Report

- The PR, and whether Step 1 rebased (old → new SHA) or verified-current, and which kind of stack
  it belongs to (planned / ad-hoc / none).
- The triage table: every comment, its verdict, and the reaction/reply posted.
- Every check: final state, classification (change-caused / lint / missing fixture / stale lockfile /
  flaky / infra), and for fixed ones the failure → fix mapping.
- Per fix: files touched, test added/updated, and the commit SHA.
- Loop rounds used (pushes) and the local `cargo fmt` / `cargo clippy` / `./test` state, naming the
  scope you ran — flag anything not green explicitly, with a visual marker.
- Items left for the user: pending disagreement approvals, out-of-scope routing, questions
  answered, flaky checks awaiting a re-run decision.

## Rules

- **Reactions are the protocol**: 👍 = agree before fixing, 👎 + reply = disagree, 🚀 = fix pushed.
  Check `viewerHasReacted` first; never double-react.
- **Never wait for the full check run to start fixing** — failures are actioned as they appear;
  `scripts/ci-status.sh --watch` is foreground work only when the fix queue is empty.
- **Stack members rebase first** — `/pr-stack-rebase` single mode before any diff is read or fix
  written; never cascade from here.
- **Say which stack model you are in.** The `pr_*` tools exist only inside a `pr-stack`
  orchestrator session; a child session working one node uses `gh` and its attached documents.
- **Route a fix to the node that owns the code.** A comment asking for a symbol listed under
  another node's `## Dependencies` is recorded against that node, never implemented here — and
  never answered with a stub, which the PR boundary contract forbids.
- **Disagreement and out-of-scope replies are user-gated** — drafted, shown, approved, then posted.
- **Never re-run, cancel, or force a check unprompted** — classify, report, ask.
- **Never resolve review threads**; never 👍 without fixing or explicitly walking it back; never
  🚀 an unpushed or partial fix.
- Fix commits contain only fix-relevant files; never `--no-verify`, never amend; batch pushes —
  every push cancels the in-flight run and starts a new one.
- Stop after the same check fails on a third consecutive push — rediagnose with the user instead
  of pushing again.
- Review summary bodies take replies, not reactions (GitHub limitation).
- **This command never merges.** No `#automerge`, no `#forcemerge`, no `gh pr merge`.

## Related

**Commands**: `/pr-stack-rebase`, `/update-pr`, `/fix-tests`, `/merge`, `/squash-pr`,
`/merge-pr-stack`, `/follow-up-branch`, `/validate-changes`, `/pr-wrap`
**Skill**: `pr-stack` (`.agents/skills/pr-stack/SKILL.md`)
**Guides**: [`docs/dev/guides/ci.md`](../../docs/dev/guides/ci.md),
[`docs/dev/guides/testing.md`](../../docs/dev/guides/testing.md)
**Specs**: [`docs/ft/coder/pr-stacking.md`](../../docs/ft/coder/pr-stacking.md),
[`docs/ft/coder/pr-stack-docs.md`](../../docs/ft/coder/pr-stack-docs.md)
