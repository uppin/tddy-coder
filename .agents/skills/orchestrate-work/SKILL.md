---
name: orchestrate-work
description: >-
  How an orchestrator agent drives a QUEUE of tasks — each becoming a PR — to green and mergeable
  through worker agents, while new tasks keep arriving. The queue may be a linear PR stack,
  independent siblings, or a dependency DAG. Defines the work-item model, line versus stored
  dependency edges, waves and concurrency caps, the per-task [G] implement / [W] validate+wrap /
  [F] fix pipeline, worker briefs and the forbidden-action list, triage of tasks arriving mid-run,
  verification of "green" claims, the back channel the orchestrator must keep open to every agent
  it starts (ask for reports, keep a watch, read before acting, treat silence as a signal, close
  the loop), the tracked work queue every delegated task lives in, the review-ready-PR deliverable
  for every change request, disk hygiene, the operator protocol, and the end-of-queue sweep with
  its TODO gate. Use when asked to orchestrate, babysit, run or resume several PRs at once, to
  feed new work into a running queue, or to supervise workers.
---

# Orchestrate work

You are the **orchestrator**. You never implement; you plan the queue, start **workers**, brief
them, verify what they claim, and keep the **operator** informed. Each **task** becomes one PR.

Mechanisms differ per harness. This skill says "your harness's equivalent of X" where a concrete
tool matters; commands such as `/green`, `/pr-wrap`, `/fix-pr`, `/validate-changes` mean those
skills **or your project's equivalent**.

## When to use / not

| Use | Do not use |
|---|---|
| Two or more tasks that each end as a PR | One PR — just run the stages yourself |
| Tasks arrive while others run | A single worker doing everything serially |
| Stack, siblings or DAG, partly done (resume) | Planning only — use the planning skills first |

In this repository a planned set of PRs is a **streak**: `/plan-streak` plans it — dependent tasks
joined into one or several linear stacks, independent ones standalone on the trunk — and
`/yolo-streak` is this skill's streak-specific form, running its pipeline while planning continues.

## 1. The queue model

A **work item** is one task:

| Field | Meaning |
|---|---|
| title | one line |
| responsibility | what this PR owns |
| boundaries | what it must not touch or implement |
| dependencies | line and/or stored edges (below) |
| branch / PR | owned branch and its PR |
| stage | G, W or F (see §2) |
| status | pending, running, parked, blocked, done |
| worker | the worker session assigned |

**Edges**

| Edge | Effect |
|---|---|
| **line** | PR base is the predecessor's branch. Lines are linear: one successor per node |
| **stored dependency** | must finish first, but the base stays trunk (or whatever it was) |

**Queue shapes**: a linear stack (all line edges), independent siblings (no edges), a DAG (stored
edges, with line edges only where code genuinely builds on a predecessor).

**Waves.** A task's earliest wave is set by its dependencies; along a line, waves never step back
(non-decreasing). Run every task of the current wave **concurrently up to the cap**. A ready task
never idles behind a neighbour it does not depend on. A task lifted to a later wave moves up.

**Track every task where others can see it — a firm rule.** Every piece of work you delegate is
recorded in a **shared work queue or checklist** that the operator and other orchestrators can
see, never only in your head or your conversation. Prefer a real work queue when your environment
has one; fall back to a plain checklist (one line per task: child, PR, status) only when it has
none.

- **Add an item** for each task the moment you delegate it.
- **Link each item to its child** when the child starts, and **to its PR** once one exists.
- **Record dependencies** between items as the queue model above defines them.
- **Update the item on every transition** — stage, status, blocker, done.
- **Read the queue before giving the operator a status**, never answer from memory.

For example, in tddy: the `work_item_*` and `work_queue_set_dependencies` tools.

## 2. The per-task pipeline

| Stage | Does | Done when |
|---|---|---|
| **[G]** implement | `/green` — make the contract's failing tests pass, commit, push | scoped gates pass, pushed |
| **[W]** validate + wrap | `/validate-changes`, `/pr-wrap` (docs, cleanup), mark ready | docs wrapped, PR ready, pushed |
| **[F]** fix | `/fix-pr` loop until the PR's **own** check rollup is green | rollup green on current head, full counts |

Rules:

- **[W] also does that task's [F]**, in the same worker.
- On a line, a task's **[F] starts only after its predecessor's [W] and [F] are both finished** —
  rebasing onto a moving base wastes CI.
- A [W] worker that must wait for its [F] is **compacted and parked**. One that moves straight into
  [F] is not compacted. **Compact before switching a worker's model.**
- **Caps**: [G] and [W] share one cap (default 2). [F] is serial bottom-up on a line and unlimited
  across independent tasks (cap counts G/W only).
- **Alternate** under cap 1 (G → W → G → W, lowest wrap first); never queue all wraps behind all
  greens.
- **Models**: mid-tier for implementation, never the smallest. Size model and effort to the task's
  complexity, and **tell the operator which and why before each start**.
- Workers run in a worktree of their own, one per task.

## 3. Worker briefs

Workers run as their own **main agent** and do **not** offload the whole job to one subagent.
Copy-paste templates: [reference/brief-templates.md](reference/brief-templates.md).

Every brief carries:

| Part | Content |
|---|---|
| Scope | responsibility, boundaries, the PR and branch |
| Base | the exact base branch and the SHA expected |
| Stage commands | the exact commands for this stage, scoped to touched packages |
| Reporting | the back channel, cadence, final format |
| Stop conditions | when to stop and report instead of improvising |
| Pre-approved / forbidden | explicit lists (below) |
| Deliverable | a review-ready PR, or a back-channel report and no PR (below) |

**A change request ends in a review-ready PR — a firm rule.** For every brief, decide first whether
the request **delivers a change**:

| The request | The brief asks for |
|---|---|
| Delivers a change (code, docs, config) | a PR **ready for review**, not a draft: docs wrapped, marked ready, checks read. The child reports the **PR number and its CI state** |
| Does not (investigation, diagnostics, a question) | a **report on the child's reporting channel** (§6) and **no PR** |
| Ambiguous | **you decide**, state the decision in the brief, and tell the operator which you chose |

A draft PR is never the end of a change request; a child that stops at a draft has not finished.

**Reporting.** A worker reports over **the worker's back channel to the orchestrator** — the
channel the brief names (§6) — before **every turn ends** and at least every ~50 minutes: a
one-line status, and at the end a structured `✅ <PR> <sha> + full check counts`. **Never let a
worker use a channel you cannot read later.** Keeping that channel open and read is the
orchestrator's duty, not the worker's: §6.

**Pre-approved** (state it): scoped build/test/lint, commits, leased pushes to its own branch,
rebasing its own branch, reading CI, opening/readying its own PR, deleting its own build outputs.

**Forbidden** (state it):

| Never | Why |
|---|---|
| merge; auto-merge and force-merge tokens | merging is the operator's call |
| force-build tokens | a second one cancels the running CI |
| dispatching workflows | dispatch runs do not count as the PR's checks |
| unstacking or re-registering a stack | breaks the stack; ask |
| force-push (only **leased** pushes, with explicit SHA) | clobbers others' work |
| killing processes by pattern | kills other workers' processes — kill only recorded PIDs |
| reading secrets (`.env`, tokens) | no need, no exposure |
| touching worktrees outside the task | operator approval required |

**"Finishing the planned scope is never a deferral."** Workers do not push planned work into TODOs.
A real discovery may be recorded as a TODO for the gate (§7), never planned scope.

## 4. Accepting new tasks mid-run

Triage every arrival into exactly one:

| Kind | Action |
|---|---|
| **New task** | its own work item: place it in the graph, compute its wave, start it when dependencies are met |
| **Fix to an open task** | route to that task's worker; fix at the **node that fails**, never at the root; **never cascade a fix through green nodes** — ask which node if ambiguous |
| **Follow-up** | record as a TODO for the end-of-queue gate; no worker yet |

Checklist for a new task: title and boundaries written; edges chosen (line only if code builds on
the predecessor); wave computed; cap and disk checked; status table re-posted.

## 5. Verification rules

| Rule | Detail |
|---|---|
| **Done = the PR's own check rollup on its current head**, with full counts | dispatch and manual runs do not count; a cancelled required run blocks merge |
| Re-run cancelled runs | marking ready right after a push can cancel that push's build |
| Verify every "green" claim against counts | a worker once claimed green while lint had failed |
| Flaky is red | fix the race in the open task that exposed it; never TODO it |
| Count-only shape tests | tests asserting "N fields/items" that break on every addition are **deleted, not bumped** |
| Behaviour tests | never deleted that way. Delete **narrowly**: one test, not the file |
| Verify every worker start | poll its start status; starts fail **silently** (worktree timeouts on a full disk) |

Draft PRs are usually not built by CI: a skipped check is **not** green. Ready the PR (after wrap)
or use the project's sanctioned build trigger **once**.

## 6. The back channel — you maintain it for every agent you start

**This is a duty, not a tip.** Every session or subagent you start has a reporting channel back to
you, and keeping it open, watched and read is **your** job from the moment the child starts until
you have relayed its final report. A child that reports into a channel nobody reads has not been
heard, and anything you tell the operator about it is a guess.

> **Why this rule exists.** On 2026-11-24 a diagnostics child sent its report twice on its
> reporting channel. The orchestrator had never subscribed to the child's messages and never read
> them, then told the operator the child had failed to report. The child had done its job. The
> orchestrator had not.

### 6.1 Ask for reports, in every brief

Every brief names **the child's reporting channel** — the exact route it reports on — and **how
often** it reports: on start, at each milestone, at once on any blocker or decision, at the end,
and at least every N minutes in between (~50 for a long [G]/[W]/[F] worker, less for short jobs).
Choose a channel **you can read later**; never let a child pick its own, and never accept one you
have no way to read.

For example, in tddy: a session reports with `tddy-tools send --to <your session id>` — on your
host or on any other connected host; a child on another host finds your host from its own start
record, so it needs no `--host`.

### 6.2 Keep listening — subscribe right after the start

**Right after starting a child**, before you do anything else, set up **a subscription or watch
that tells you when the child messages or ends** — one that wakes you even while you are idle
between turns. **Never rely on remembering to poll.** Check that the subscription exists, and
renew it before it expires while the child lives.

For example, in tddy: `CreateSessionWatch` on `message` and `ended`, with `wake_when_idle` —
`started_by_me` covers the children you started on every connected host, and each event names the
host it came from (`daemon_instance_id`).

### 6.3 Read before acting

Before you **answer the operator about a child**, **ask a child anything**, or **conclude that a
child is silent, stuck or failed**, **read the child's messages and its transcript**. A
notification says *that* something arrived; the messages and the transcript say *what*. Never state
a child's status from memory or from the last notification you happened to see.

For example, in tddy: `ReadSessionMessages` and `ReadSessionTranscript`.

### 6.4 Silence is a signal

A child that **misses its reporting interval** gets checked at once: its transcript, its terminal
and its recent activity. Then **tell the operator** what you found — still working, waiting for
input, stuck, ended. **Never assume a silent child is still working.** If it ended, resume or
restart it; if it is waiting, answer it.

An idle "waiting for input" right after a status report is **normal** — that is the child doing
what it was told, not silence.

### 6.5 Close the loop

When a child **ends**, read its **final report** (messages and transcript), **relay it to the
operator**, and **only then** drop its subscription. A subscription dropped before the final report
is read loses the one message that mattered.

### 6.6 Keep-warm and CI watching

- **Heartbeat**: besides the subscriptions, a long fallback — a keep-warm check about every 10
  minutes. If the active [F] worker has had no activity for ~50 minutes, treat it as §6.4.
- **Exactly one CI watcher per PR.** A timed-out foreground watch keeps running and burns the
  shared API rate limit; never start a second.

## 7. The sweep and the TODO gate

Once pipelines drain, plan a final **sweep task** on the tail: it closes the code issues the queue
added — oversized files, fixable TODOs, dead helpers — in place. TODOs **still open after it** go
to the operator as a **decision gate** (do now, defer with a record, drop). The sweep runs the same
pipeline and verification as any task.

## 8. Resources and hygiene

- **Disk is the usual bottleneck**: a build per worktree is 20–40 GiB. Keep a **free-space floor**
  (e.g. 15 GiB) and check it at **every transition**.
- Scoped builds and tests, cheaper build settings, clean a package after its tests pass; delete
  idle workers' build outputs.
- **Never touch worktrees outside your queue** without operator approval. Ask; meanwhile have the
  worker shrink its own footprint and push to CI instead of verifying locally.
- **Per finished task**: end the worker session, then delete its worktree — only when it is clean
  and pushed. Never clear a worktree in use or with uncommitted/untracked files.
- Delete stray artifacts (test screenshots) before committing.

## 9. Operator protocol

| Situation | Do |
|---|---|
| Operator away | do not ask; take the recommended option at **pre-approved** forks; record each decision |
| Not pre-approved (others' resources, merging, a hand move the tooling refused) | **park** with a notification; do not do it |
| Notifications | only on completion or a real block — never routine progress |
| Every stage transition | update the work queue (§1), then re-post the status table: **task, PR, stage, state, head SHA** |
| Trunk moves (earlier tasks merged) | refresh, check which open tasks need a rebase, tell those workers |

**Final report**: what is green (with counts), decisions taken on the operator's behalf, cleanup
done, open decisions (TODO gate, approvals, the merge).

## 10. Common failure modes

| Failure | Fix |
|---|---|
| **Silent start failure** | poll start status after every start; on failure check disk and worktree creation, restart |
| **False green** | recount from the PR's check rollup; send the worker back to [F]; never relay an unverified claim |
| **Over-broad deletion** | restore from git; re-delete narrowly (one test); only count-only shape tests are deletable |
| **Pattern-kill** | forbid it in every brief; kill only recorded PIDs; check other workers' state after any incident |
| **CI cancelled by ready-after-push** | re-run the cancelled runs; ready only after the push build starts, or re-run |
| **Disk full** | stop new starts, delete idle builds, shrink workers, ask before touching foreign worktrees |
| **"The child never reported"** | it probably did: read its messages and transcript before saying so; a missing subscription is your defect (§6) |
| **Rebased base / queue merged mid-run** | refresh trunk, recompute which tasks sit on a stale base, rebase bottom-up (≤2 PRs building at once), repoint bases **before** pushing rebuilt branches, tell affected workers, re-verify rollups |
