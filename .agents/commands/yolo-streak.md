---
description: /plan-streak with an implementation pipeline bolted onto pass 2 - as soon as a node's contract commit is pushed and its behaviour needs are delivered, give it an ephemeral worktree and run /green, then /validate-changes + /pr-wrap, then a /fix-pr loop - [G] and [W] in per-node subagents sized to the node, [F] as ONE long-lived serial fixer working in one reusable worktree - and remove each per-node worktree once [W] has pushed. Works on the whole streak at once - several gh stacks plus standalone PRs - with waves and readiness taken from the dependency graph, not from one line. A **sweep node** is planned at the tail of each stack once the pipelines drain: it solves every code issue the stack added and every TODO it can, and the TODOs the streak still leaves open are put to the user. Planning (interview, pass 1, the 6e acceptance-test review) stays interactive. Pointed at a streak that is already partly done, it **resumes**: it reads each node's implementation status from git, GitHub and CI and starts every node at the stage it has not finished. Every subagent is started with an explicit model **and** effort taken from the changeset's complexity score, and the orchestrator tells the user which and why before each one starts
---
## YOLO Streak — `/plan-streak`, and every ready node implemented while pass 2 keeps going

`/yolo-streak` is `/plan-streak` with one extra mode in **pass 2**. Run
`.agents/commands/plan-streak.md` end to end — interview, the streak-map approval, pass 1, pass 2's
per-node 6e acceptance-test review and 6h link revision, stack registration, the completion gate — and
add the **pipeline** below at the two points named in *Hooks into pass 2*. Nothing in `/plan-streak` is
skipped, shortened or relaxed. The pipeline only starts work that pass 2 would otherwise leave for the
hand-off in its Step 11.

A streak is a **set** of PRs: one or several `gh stack` stacks (each linear, each governed by the
`pr-stack` skill) plus **standalone** PRs based on the trunk (`/plan-streak` § *The streak model*).
The pipeline treats every node alike; what differs between a stacked and a standalone node is only
**lineage** — a stacked node has a predecessor whose head it must contain, a standalone one has none.
The general orchestration model this command applies — the queue, line versus dependency edges,
waves, caps, briefs, verification — is the `orchestrate-work` skill's; this file is its
streak-specific form.

```
pass 2 lands nₖ (contract commit pushed) → moves on to nₖ₊₁
        └─ nₖ ready? ──► ephemeral worktree ──► [G] /green ──► [W] /validate-changes + /pr-wrap ──► worktree freed once pushed
                                                  [G] and [W] = per-node background subagents, model sized to the node
        serial [F] fixer (ONE agent, ONE reusable worktree) ── next eligible node in streak order: /fix-pr until green ── F ✅ ──► next node
```

`[G]` and `[W]` are per-node subagents, as ever. `[F]` is **one long-lived agent for the whole
streak**, working in one reusable worktree and moving from node to node in streak order: it keeps its
context across nodes (the streak's conventions, earlier answers, what already failed in CI), which is
why it replaces the per-node continuation of `[W]`'s agent. The `[G]` lane is capped at 1 and the
fixer is 1. There is **no fan-out inside a stage**: a stage agent does not spawn helper subagents.

```
pipelines drain → per-stack cascade → collect each unit's open TODOs + code issues
                → plan a SWEEP node at the tail of each stack that left any → their pipelines
                                                  └─ TODOs still open ──► user: continue the streak | defer
```

The orchestrator is the session running pass 2. It owns the worktrees, picks each subagent's model,
starts the next stage when the previous one reports, and keeps the interactive parts interactive.
Subagents cannot ask the user anything; each one runs under **pre-approved consent** (below) and
stops with a 🔴 report where that consent runs out.

**Progress notes.** Every `[G]`, `[W]` and `[F]` agent sends the orchestrator a ~5-line note with
`SendMessage` to `main` about **every 50 minutes**: its head, what changed since the last note, what it
waits on, and the stops it can see coming. The orchestrator relays the notable ones to the user. A stage
agent that goes **quiet** — typically after its own helpers finish and nothing woke it — is checked
(`git`, `gh`, its worktree) and resumed with `SendMessage`, never assumed dead.

## Resume mode — a streak that is already partly done

Run this command at a streak that already exists — the user gives a PR in it, the streak slug, or
says "resume" while a streak branch is checked out — and it does **not** re-plan. It reads where every
node actually stands and starts each node at the first stage it has not finished. Nothing that is
already done is redone, and nothing is trusted that cannot be seen.

**Evidence, not memory.** A ledger file in the scratchpad is a *hint* at what happened; git, GitHub and
CI are the truth. If they disagree, the evidence wins and the final report names the disagreement.

### Step R1 — locate the streak, its units and its edges

- **Nodes**: every PR whose head is `feature/<slug>/…`
  (`gh pr list --state all --json number,headRefName,baseRefName,isDraft,state --search "head:feature/<slug>/"`),
  plus — when this session owns the streak's work queue — every item `work_queue_graph` lists, which
  also names the nodes that have no PR yet.
- **Units**: a node based on the trunk with nothing based on it is standalone; a chain of PRs based on
  each other is a stack, and `gh stack view` (after `gh stack checkout`) confirms it is registered. A
  chain that is **not registered** is a gap pass 2 left: register it (`/plan-streak` Step 7) before
  dispatching, and say so.
- **Edges**: each node's changeset `## Dependencies` and `## Green wave` at its branch head
  (`git show origin/feature/<slug>/<node>:docs/dev/1-WIP/<file>`, or the wrapped docs if it is already
  gone from `1-WIP/`) — the same source Dispatch uses. A queue's `needs` must agree; a disagreement is
  reported, and the changeset wins.

### Step R2 — derive each node's status

Read-only, from the orchestrator, one node at a time. **First row that matches wins:**

| Evidence on the node | Status | Starts at |
|---|---|---|
| PR merged | `landed` | nothing — counts as delivered. If a successor is still based on its branch, say `/repoint` is needed; this command does not do it |
| PR ready (not draft), changeset gone from `1-WIP/` (wrapped), the **PR's own check rollup** terminal-green and every thread addressed **on the current head SHA** (see *What counts as green*) | `done` | nothing |
| PR ready and wrapped, but a check is red or running, or a thread is open | `F` | `[F]` |
| Tests green and pushed, PR still draft or its changeset still in `1-WIP/` | `W` | `[W]` |
| Contract commit pushed, commits after it, tests **still red** | `G:resume` | `[G]`, told what exists |
| Contract commit pushed, nothing after it | `G` | `[G]` |
| Branch and PR exist with commit 1 only, no contract commit | `plan:pass2` | pass 2 for this node from 6c (**6e stays interactive**) |
| Planned — a draft in the planning worktree's `1-WIP/` or a queue item — but no branch | `plan:pass2` | pass 2 for this node from 6a |
| Not planned at all | `plan:pass1` | `/plan-streak` from Step 4 |
| Contract commit carries no failing test (see *Which nodes qualify*) | `manual` | left for the Step 11 hand-off, as in Dispatch |

**"Tests green" comes from CI first, but drafts are not built.** A terminal-green test check on the
head SHA is the evidence, and it costs nothing — but a draft has none unless someone commented
`#forcebuild` ([ci.md § Draft PRs](../../docs/dev/guides/ci.md#draft-prs)), so for `W`-looking drafts
expect to probe. Probe by creating the node's ephemeral worktree (pipeline step 0) and running its
changeset's verification **scoped** to the packages it owns (`./test -p <pkg>`). Keep that worktree if
the node goes on to `[G]` or `[W]`; remove it if the node turns out `done`. A node with **commits
after the contract commit but no CI and no probe result** is `G:resume`, never `W` — a `/green` that
finds nothing red stops, so a wrong `W` is cheaper than a wrong `G`, and a wrong guess in either
direction is not acceptable; probe. Do not post `#forcebuild` to get evidence: that is a CI run per
node this command did not ask for.

A branch **pinned** by another worktree is `stopped:0` (somebody is working on it), exactly as in step 0.

### Step R3 — show the table, then dispatch

Print one table: node, PR, unit, status, the evidence that decided it (a SHA, a check name, a thread
count), the complexity score, and the stage it starts at. Then run Dispatch as normal, with these
changes:

- A node's **ledger row begins at its derived stage**, not at `queued`. `G:resume`, `W` and `F` rows
  start at that stage's subagent; a node ahead of the others never waits for them.
- A dependency counts as **delivered** when it is `W`, `F`, `done` or `landed` — its `[G]` happened.
- Each stage brief for a resumed node adds the facts the stage needs and says: **re-verify, do not
  redo.** `G:resume` gets the head SHA, the commits after the contract commit and the tests still
  failing, and builds on them. `W` re-runs its own validations rather than trusting that green was
  proven. The serial `[F]` fixer starts **fresh** on a resume, sized by the highest `[W]` row among the
  nodes it will walk, and first reads each diff and changeset as it reaches it.
- **Nodes with `plan:` statuses are planned first**, interactively, because their acceptance tests
  need the user. The implementation pipelines of nodes already past their contract commit start
  alongside, as in the ordinary command — planning never waits on them and they never wait on it.
- If a stack already has its **sweep node**, it is an ordinary node with these statuses. If not, it is
  reserved and planned after the pipelines drain, as usual. The TODO gate shows only TODOs with no
  recorded consent; an entry that already carries a `defer` consent line is not asked again.
- Stop and ask only where the evidence is ambiguous (a probe that fails to run, a head that moved
  while reading). Do not stop to confirm the table.

The rest of the command — consent, stops, the completion gate, the sweep — is unchanged.

## Sizing every subagent — model and effort, always stated

**Every subagent this command starts gets an explicit `model` and an explicit `effort`.** No stage
runs on a default. The command's own instruction is what authorises passing `effort` on the Agent
call. The one exception is where effort cannot be set — an agent continued with `SendMessage` keeps the
effort it started with, and Cursor has no effort knob — and there the ledger and the announcement say
`inherited` or `n/a`, not nothing.

**Haiku may be set up to `xhigh`.** Effort is how a small node on a small model gets enough depth;
reach for a larger model only after the effort ladder is spent.

**The input is the changeset's `**Complexity**: N/5` header line** (scored at planning, see
`.cursor/rules/changeset-doc.mdc` § Header Metadata, re-scored in the contract commit). A changeset
without one is scored now from that rubric and the announcement says so. The orchestrator may move
**one row** off the score when the contract commit contradicts it (far more failing tests, or more
packages, than planned) — never silently, and never more than one row.

| Complexity | `[G]` `/green` · `/code-restructuring` | `[W]` `/validate-changes` + `/pr-wrap` |
|---|---|---|
| **1** | Haiku · low | Sonnet · low |
| **2** | Haiku · medium | Sonnet · low |
| **3** | Haiku · high | Sonnet · medium |
| **4** | Haiku · xhigh | Opus · low |
| **5** | Sonnet · medium | Opus · medium |

- **`[G]` retry.** A stage that returns 🔴 because the model was out of its depth may be retried
  **once, one row up** (score 1 → 2 … 4 → Sonnet · medium). Sonnet · medium is the ceiling. A node that
  seems to want more has a boundary problem: the stage stops and says so rather than escalating
  silently. Record both attempts.
- **`[W]` raise.** `[W]`'s effective score is the complexity **+1** (cap 5) when `[G]` needed its
  retry, a file this PR grew has to be decomposed (`pr-wrap` step 3.5), or the code-issue gate found
  added issues. Opus · medium is the ceiling.
- **`[F]` is one serial agent for the whole streak**, so its model and effort are chosen **once**, from
  the **highest** `[W]` row among the nodes it will walk. A replacement (context exhausted, harness
  restart, a reopened layer) is announced like any start and resumes at the node the ledger names.

### Announce the choice — every time a subagent starts

Before each Agent call, in the orchestrator's own text, **one line to the user** saying what was chosen
and why. One line per subagent, never batched, never skipped because several nodes dispatch at once.
It names the node, the stage, the model · effort, and the **reason** — the complexity score and the
signals behind it:

```
▶ n3 #702 [G] /green → haiku · xhigh — complexity 4/5 (two crates, proto change); first attempt
▶ n1 #700 [W] /validate-changes + /pr-wrap → sonnet · medium — complexity 2/5 raised by 1: [G] needed its retry
↗ n3 #702 [G] retry → sonnet · medium — haiku · xhigh stopped 🔴 out of its depth on the proto seam
▶ streak [F] /fix-pr walker → sonnet · medium — highest [W] row in the streak is sonnet · medium; serial, one worktree, starts at n1 #700
▶ n5 #705 [G] /green → haiku · high — complexity 3/5 moved up one row: contract commit has 14 failing tests, plan said 6
▶ n7 #707 [G] /green → haiku · low — complexity 1/5; standalone, wave 1
```

The same model, effort and reason go in the ledger row. A user who disagrees can say so on the spot;
the next start uses their answer.

## What stays interactive

Planning is the user's call and this command does not change that: the interview, the streak-map
approval, and **6e — the per-node acceptance-test review**. A node's contract commit is pushed only
after its 6e approval, so a pipeline never starts on tests the user has not seen. A **6h link
revision** that needs a decision (`/plan-streak` 6h: a dependency across units) is interactive too.

## Hooks into pass 2

**Hook 1 — after 6g and 6h, once the planning worktree has left nₖ.** The branch is pinned only while
it is checked out here, so the order is: finish 6g and 6h for nₖ → cut nₖ₊₁ (its 6a detaches onto
`origin/<base>`), or, for the last node, Step 7 then Step 8's restore of `$ORIGINAL_BRANCH` → *then*
run the **dispatch** below. Never create the worktree for nₖ while this worktree still has it checked
out; `git worktree add` refuses, and the workaround (`--force`, detaching) is the pinning problem
`/plan-streak` exists to avoid.

**Hook 2 — whenever a pipeline's `[G]` stage reports green.** That can unblock nodes whose contract
commit is already pushed. Run the dispatch again. It also re-runs after every pipeline finishes, to
refill the concurrency cap.

**A green pushed by a pipeline moves a stack under pass 2.** `/plan-streak` 6a cuts each stacked node
from its predecessor's **remote** tip, so a node cut after its predecessor went green simply starts on
the green. That is expected, not a leak — 6g's *exactly TWO commits* check counts commits above the
base's **current** tip, whatever that tip now holds.

**A link revision moves the pipelines too.** When 6h changes a node's placement or needs, re-read the
ledger against the new links before the next dispatch: a node that became standalone loses its
lineage rule, a node that gained a behaviour need waits on it, and a node whose branch 6h rebuilt is
`stopped` for any pipeline that had it checked out — a rebuild under a running pipeline is a stop, not
a race to win.

**Stack registration and running pipelines.** Step 7 registers each stack once its members all have
contract commits; pipelines may already be running on them. Registering moves no branch and needs no
worktree, so it does not wait for them.

### Dispatch — which nodes start

A node is **ready** when all of these hold:

1. Its contract commit (commit 2) is pushed and its 6e was approved.
2. It has no **started** pipeline: its ledger row is absent, `queued` or `blocked:n<j>`. A row in
   `G`, `W`, `F`, `done` or `stopped` is excluded. (`F` rows are served by the serial fixer, not
   dispatched here.) (In resume mode a row is created at the node's derived stage — see *Step R3*.)
   `queued` and `blocked` rows are **re-evaluated by every dispatch** and move to `G` the moment rules
   1 and 3–5 hold, which is how a node waiting for a free slot or a resumed blocker ever starts.
3. **Every node it has a behaviour need on has delivered `/green`.** The needs are the real edges its
   changeset's `## Green wave` lists — the committed source, current after 6h (pass 2 can add or drop
   one); with a work queue, `work_queue_waves` reports the same as each item's *binding needs*. Nodes
   that are only below it in its stack, or only next to it in streak order, do not count. A dependency
   has delivered when its `[G]` stage reported ✅ with the head pushed. **While any needed node's `[G]`
   is queued, in progress, or being retried, this node waits, even with a free slot and a pushed
   contract commit.** Its worktree is not created, and no subagent starts. Wave 1 nodes — every
   standalone node among them — have no needs. `/green` would stop at its dependency gate (`green.md`
   0b) anyway, so an early start is only a wasted worktree.
3b. **A stacked node's branch already contains every delivered dependency.** For each dependency —
   which, under `/plan-streak`'s placement rules, sits below it in the same stack — its pushed head
   must be an ancestor of this node's branch:
   ```bash
   git merge-base --is-ancestor "origin/feature/<slug>/<dep>" "origin/feature/<slug>/<node>" \
     || echo STALE
   ```
   A green that lands below a node leaves every layer between them stale. A node dispatched on that
   lineage stops at `/green`'s dependency gate having done nothing, which wastes a slot and a
   worktree. When the check prints `STALE`, mark the row `blocked:lineage`. Then cascade
   `/pr-stack-rebase` from the lowest stale layer up to this node, but only if none of those layers
   is pinned and none has unpushed work. If one is, wait for its pipeline. Run the check again
   before dispatching. A **standalone** node has no lineage to check: its base is the trunk, and a
   moving trunk is not a reason to hold it.
4. No node it needs is in a 🔴 stopped state. Mark it **blocked**, not failed, and say which node
   blocks it.
5. The running count of `[G]` agents is below the **`[G]` lane's cap**.

**Lane caps: `[G]` 1, serial `[F]` 1**, unless the user set other numbers in this request. A node's
`[W]` follows its own `[G]` in the same per-node worktree, so it adds no lane. Each worktree builds its
own Rust `target/` (several GB per worktree, more with `tddy-web` deps), every green that lands on a
stacked node makes the layers above it stale (`/plan-streak` Step 10 § *What concurrency costs*), and
parallel cargo builds compete for the machine. Dispatch is **proactive**: `[G]` always takes the next
ready node in **streak order**, blockers first, as dependencies deliver, so its lane never idles while
a ready node exists — whichever stack it is in, or none. Say the caps in the Step 1 streak-map approval
so the user can change them before anything is created. Say there too that a **sweep node** will be
appended at the tail of each stack that leaves debris (see *The sweep node*), with its scope unknown
until the streak has been implemented. Nodes that are ready beyond the cap queue in streak order,
blockers first.

**Lineage after parallel greens.** Greens and wraps rewrite stacked branches, so every next stage on a
stacked node starts with the single-mode rebase onto the predecessor's current head
(`git rebase --onto origin/<predecessor> <old base tip>` — a plain rebase replays the predecessor's old
commits once it was rewritten). When a span of a stack was **built on a node's contract rather than
its green**, run a **rebase-only cascade** over that stack before dispatching `[G]` above it: one
agent, no CI waits, scoped gates per branch, and the lower node's design wins on a duplicate helper
move. Standalone nodes never need one.

**Re-assess the links after each pass, and after every `[G]` that reports.** A green can settle an
edge that the contract only guessed at — the same evidence `/plan-streak` 6h acts on. Re-run
`/plan-streak` Step 8b's checks: the placement rules hold, each stack's waves never step back, ties go
to the earliest dependencies, and the labels count up. If a check fails, **stop dispatching above the
affected layer** of that stack (or the affected standalone node), say so, and fix it once the
pipelines on those branches have drained. A reorder or a re-placement rewrites branches the pipelines
have checked out. Other stacks and standalone nodes carry on.

### Which nodes qualify

**Only nodes whose contract commit has something red qualify for this pipeline.** `/green` stops when
no test is red, and a node whose commit 2 is a recorded baseline (a mechanical move, toolchain or CI
change, a test rebalance whose characterisation tests are already green) has none. So: nodes whose
contract commit carries **failing tests** run the pipeline, and **Refactor** nodes (`Type: Refactor`,
planned through `code-restructuring` with a `plan.jsonl`) run it with `/code-restructuring` as their
`[G]` implementer. Every other node gets ledger status `manual` and is left for the `/plan-streak`
Step 11 hand-off. Its dependents wait as `blocked:n<j>` until a human finishes the node, and the final
report says so. A changeset that names a different implementer wins.

### Ledger

Keep one file in the scratchpad: `yolo-streak-<slug>.md`, with **one row for the serial `[F]` fixer**
(agent id, model · effort and why, worktree path, the node it is on, last progress note) **plus one row
per node** — label, branch, PR number, **unit** (stack name and position, or `standalone`), its
behaviour needs, per-node worktree path (none once freed), stage (`queued · G · W · F · done ·
stopped:<stage> · blocked:n<j> · blocked:lineage · manual`), the complexity score, **model · effort
used per stage with the reason** (both attempts, when retried), head SHA at the last report. Update it
on every report and on every 6h revision. It is the orchestrator's memory across a context reset:
after one, re-derive stages from `git worktree list`, the branches' pushed state and each PR's checks,
never restart a node whose row is past `queued`/`blocked`. The ledger is not committed. The work queue,
when there is one, is the shared record of the links; the ledger records the pipelines.

## The pipeline, per ready node

### 0 — Ephemeral worktree

```bash
W="$(dirname "$PWD")/yolo-<slug>-<node>"                 # a sibling; named, never numbered
git fetch origin "feature/<slug>/<node>"
git worktree add "$W" "feature/<slug>/<node>"            # must succeed: nothing pins the branch
git -C "$W" status --porcelain | grep -v '^?? '          # must be empty
git -C "$W" rev-list --count "origin/feature/<slug>/<node>..HEAD"   # must be 0
(cd "$W" && ./dev cargo build -p <pkg>)                  # each package in scope; never a workspace build
(cd "$W" && ./dev bun run local-registry-install)        # only if the node touches a web package
```

Run the builds in the background; the subagent waits on the warm `target/`, not the orchestrator. If
the add is refused because the branch is pinned, **stop that node** and mark it `stopped:0` —
somebody is working on it. Do not `--force` and do not make a second copy.

### Binding a subagent to the worktree

The Agent tool has no working-directory parameter, and `isolation: "worktree"` would cut a fresh
branch, which is the opposite of what is wanted. Bind in the prompt, and say all of this:

- **First action:** `cd <W>` and confirm `git rev-parse --show-toplevel` is `<W>` and
  `git branch --show-current` is `feature/<slug>/<node>`. Stop if either differs.
- Every file path is absolute and under `<W>`. Every Bash call starts with `cd <W> &&`, because the
  shell may not persist. Never touch the planning worktree or any other worktree.
- Work **this branch only**: no `gh stack sync`, no `gh stack rebase`, no `gh stack submit`, no cascade
  rebase. `/pr-stack-rebase` in single mode is the only rebase. Never amend, never `--no-verify`.
- Say whether the node is **stacked** (its predecessor's branch is its base, and successors are based
  on it) or **standalone** (its base is the trunk, nothing is based on it).
- **Scoped gates only** — `./test -p <pkg>`, `cargo clippy -p <pkg> -- -D warnings`, the single spec
  under change; never a bare `./test` or a full Cypress run
  ([AGENTS.md § Verification](../../AGENTS.md#verification-scope-it-locally-run-it-in-full-on-ci)).
- **Commit and push** before reporting — a stacked branch is a successor's base, and an unpushed
  standalone one is invisible to CI and to the orchestrator.
- Report in the format at the end of this file, and nothing else.

Fork-style context is not needed and costs tokens. Use a **fresh** subagent for `[G]` and a **fresh**
one for `[W]`, and one long-lived agent for `[F]` (below; bound the same way to its own worktree), with
the brief above, the PR number, the changeset path (or the walk, for `[F]`), and the command to run.
Name the stage's consent section below; do not paste the whole of this file.

### Stage G — `/green`

Run `.agents/commands/green.md` end to end in `<W>` (or `/code-restructuring` for a Refactor node).
`/green` delegates to the `tdd-implementer` subagent itself; the stage subagent's model is what
drives it. Consent is `green-yolo.md` § *Consent*, the `/green` row: commit the green work, relevant
files only, and push. The same **not pre-approvable** stops apply: nothing red, a planned-scope
deferral, a stack gate (the dependency gate, a leak in `origin/<base>..HEAD`, unplanned deletions),
a claimed code issue, a refused restructure, tests or clippy still red.

**Model and effort.** From the changeset's complexity score — *Sizing every subagent* above. Pass both
`model` and `effort` on the Agent call and announce the choice first.

### Stage W — `/validate-changes` + `/pr-wrap`

Start only when `[G]` reported green and pushed. Run `.agents/commands/validate-changes.md`, then
`.agents/commands/pr-wrap.md`, end to end in `<W>`, answering their questions from `green-yolo.md`
§ *Consent* — gaps from `/validate-changes` go back to `/green` **once**, inside this stage; apply
every cleanup, Minor included; decompose files this PR grew past 500 production lines; solve every
code issue this PR added; take your own recommendation per TODO; wrap docs, retitle, `gh pr ready`.

**One deliberate difference from `/green-yolo`: stack-shared files.** `green-yolo` splits a file that
other stack PRs also modify and then cascades a rebase over the whole tail. That cannot run here —
sibling pipelines have those branches pinned. So on a **stacked** node, `pr-wrap` step 3.5's own
stack-branch rule applies: **do not split a file other nodes of its stack also modify**; record it as
`pr-wrap` says and list it in the report. The stack's sweep node splits it later. Every other file is
decomposed now, as `green-yolo` requires. A **standalone** node has no successor to cascade over, so
it decomposes its files as `green-yolo` does — and reports any file it split that another open node of
the streak also modifies, since that node will meet the split when it rebases or merges.

**One deliberate difference from `/pr-wrap`: the top node's backlog sweep.** `pr-wrap` step 8 asks the
top node of a stack to judge the stack's `docs/dev/todo/` delta and offer *Extra node* verdicts. Under
this command that question belongs to *The sweep node* and its TODO gate, so `[W]` lists the delta in
its report and does not ask.

**Model and effort.** `[W]`'s row for the node's complexity, raised by one when the *Sizing* rule says
so — pass both `model` and `effort`, announce first.

### Stage F — `/fix-pr`, looped, serial

**One long-lived `[F]` agent for the whole streak**, in **one reusable worktree** (`yolo-<slug>-fix`).
It takes the **first eligible node in streak order** and runs `.agents/commands/fix-pr.md` on it,
**repeatedly**, until both hold on the **current head SHA**:

- every check in **the PR's own rollup** is terminal and passing (`scripts/ci-status.sh <N>`; readying
  the PR in `[W]` is what started its build). See *What counts as green*: a passing `workflow_dispatch`
  run next to a cancelled `pull_request` run is **not** green. And
- every review thread is addressed — the main (non-nit) comments fixed or answered with evidence, per
  `fix-pr.md`'s reaction protocol.

A node is **eligible** when its `[W]` is done and, for a **stacked** node, its predecessor is `done`
(the fixer rebases onto that predecessor's final head). A standalone node and a stack root are eligible
as soon as their own `[W]` is done.

Then it sends the orchestrator `F ✅ #<N> <sha>` and **moves on**: worktree clean and nothing unpushed →
`rm -rf target` → `git switch` to the next eligible branch → for a stacked node, single-mode rebase
onto its predecessor's current head (`git rebase --onto origin/<predecessor> <old base tip>`; a plain
rebase replays foreign commits once the predecessor was rewritten) → scoped gates → push with an
explicit lease → the `/fix-pr` loop. A standalone node is not rebased onto a moved trunk unless its PR
conflicts with it. A branch checked out elsewhere cannot be switched to, so the orchestrator **frees a
node's per-node worktree** (only if clean and fully pushed — step 3's checks) before the fixer needs
that branch.

One `/fix-pr` run already re-checks running checks and fixes failures as they appear, so the loop is
for what it hands back: a new push that restarts CI, a new comment. Between rounds the agent
re-reads the PR state; it does not sleep-poll. Consent is `green-yolo.md` § *Phase C* — including its
stops, which end that PR's loop with a 🔴 and **never** re-run a flaky check unprompted:

- a flaky or infra failure that needs a re-run → stop, ask;
- the same check failing on a **third** consecutive push → stop;
- a fix for one failure that keeps causing another → stop;
- **a round cap of 5, per PR.** A fifth round with the PR still not green is a stop, not a sixth try.

A stopped PR blocks the fixer **for the nodes above it in its stack** — everything there needs it —
and nothing else: the fixer moves on to the next eligible node in another stack or a standalone one.
The report says which nodes the stop holds.

**CI traps worth knowing.**

- A `workflow_dispatch` on a PR branch **cancels the PR-event run** of the same workflow when their
  concurrency group ignores the event. Key groups on `github.event_name`.
- GitHub may report a very large PR as `mergeable_state=dirty` with no real conflict and start **no PR
  checks**. Dispatching the workflows on the branch is the fallback, and it needs the user's consent
  (not pre-approved).
- Re-running runs that were **cancelled by our own dispatches** is not a flaky re-run.

**This command does not merge.** That is `/green-yolo` Phase D, and nobody invoked it here. Landing
stays with `/merge-pr-stack` for each stack and with an ordinary merge for each standalone PR.

**Model and effort.** The highest `[W]` row among the nodes it will walk (see *Sizing*); an agent
cannot change either once started, so there is no per-node escalation. A check that keeps failing or a
thread arguing a design point is what the round cap and the stops are for: the stage stops and
reports. If the agent is no longer resumable (context exhausted, harness restarted), start a fresh one
with the same brief at the node the ledger names, announce it, and say so in the ledger.

### What counts as green — the PR's own checks, never the run list

A node is green, and may be marked `done`, only when **the PR itself** says so on its current head:

```bash
gh pr view <N> --json headRefOid,isDraft,mergeStateStatus,statusCheckRollup \
  --jq '"\(.headRefOid[0:9]) draft=\(.isDraft) merge=\(.mergeStateStatus) not-green=\([.statusCheckRollup[]
        | select(((.conclusion // .state) | IN("SUCCESS","SKIPPED","NEUTRAL")) | not)] | length)"'
# or: scripts/ci-status.sh <N>
```

`draft=false`, `not-green=0`, and `mergeStateStatus` not blocked by checks. Two shortcuts look green and
are not:

- **`gh run list --branch … --limit 1` per workflow.** GitHub's merge gate reads the checks of the
  `pull_request`-event runs on the head. A `workflow_dispatch` run on the same SHA can pass while the
  PR's own run sits **cancelled**, typically because a force-push or a newer push cancelled it through
  the concurrency group. The run list then shows success, but the PR stays BLOCKED, and every PR above
  it in its stack reads *Blocked downstack*. A manual dispatch is diagnostic, never the gate.
- **A draft's run.** A draft is not built (`docs/dev/guides/ci.md` § Draft PRs): every job is skipped,
  so its "success" proves nothing.

**A cancelled `pull_request` run on the head is not a flaky failure.** Re-run it
(`gh run rerun <id>`) without asking: it is the run the PR is gated on, and it never ran. This is
distinct from `/fix-pr`'s rule against re-running a *failed* check unprompted, which still holds. After
every push or force-push the orchestrator makes, list the head's `pull_request` runs and re-run any that
were cancelled before reading the result.

### Keep the ready lineage clean — a done layer is frozen

Every rewrite of a stacked layer re-runs CI on it and makes every layer above it stale. So a cascade
from the bottom rebuilds the whole stack, and each rebuilt PR has to go green again. Once a node is
`done`, its head is **frozen** — stacked or standalone:

- **Fixes found later go up, not down.** A problem found while working on n_k that lives in the code of
  a done node n_j is fixed in **n_k**, the open node that exposed it, or recorded for the sweep node. It
  is not pushed into n_j. Rewriting n_j needs the user's go-ahead, and the ask names every layer it
  would rebuild.
- **Rebase only the layer whose predecessor moved.** When an open predecessor pushes, rebase the open
  layers of its stack that need it (`/pr-stack-rebase` single mode, or a cascade starting at the first
  open layer). Never start a cascade at a done layer.
- **No cascade onto the trunk while the lower layers are ready.** Rebasing a stack onto a moved trunk
  rewrites every layer. Do it only when a layer actually conflicts with the trunk or cannot build on
  it, or when the user asks. Even then, it starts at the lowest layer that needs it, and the report
  says which layers will be rebuilt. The same holds for a standalone PR: rebase it onto the trunk only
  on a conflict or the user's ask.
- **Check the lineage before marking a stacked node done.** Each layer must contain its predecessor's
  current head (`git merge-base --is-ancestor origin/<predecessor> origin/<layer>`). A done layer whose
  predecessor's head is not in it is not done.

### Models on Cursor

The Haiku / Sonnet / Opus tables above are the **Claude Code** choice. On **Cursor**, every stage runs
on Cursor's default subagent model for every node whatever its size — Cursor has no per-node model
ladder to size against, so the complexity rubric does not choose a model or effort there. Still
announce each start, with the complexity score and `effort n/a`. Binding to the worktree, the stage
order, the consent and the stops are identical on both harnesses.

### Muse mode (`#muse`)

When the user asks for it — `#muse`, or in words ("let Muse do it", "offload to Muse Code") — every
implementation stage is done by **Muse Code run headless through its programmatic CLI**
(`muse exec --json`, one session per stage name continued with `--session-id`), driven by a
Claude Code subagent, through the **`muse-subagent` skill** (`.agents/skills/muse-subagent/SKILL.md`).
Read it first. **Only `[G]`, `[W]` and `[F]` are offloaded.** Planning stays with the orchestrator's
own model: the interview, the streak-map approval, pass 1, all of pass 2 (changesets, PRDs, the
contract commits with their failing tests, 6e, the 6h link revisions, stack registration), the sweep
nodes' planning and the TODO gate. The dispatch rules, the ledger, the consent and the stops are
unchanged. What changes:

| | Default | `#muse` |
|---|---|---|
| Step 0 worktree | `git worktree add` beside the planning worktree, warm builds | the same |
| Who edits code | the stage subagent | Muse Code, in the node's worktree |
| Stage subagent | does the stage, model by the rubric | a **driver**: Sonnet · low (effort passed explicitly), briefed with `muse-subagent` `references/driver-brief.md` and the stage's prompt from it |
| Model doing the work | the G/W rubric | `MUSE_MODEL` from `muse-agent.sh ensure local` (or the one the user named), every stage, every node. No retry one row up — a `[G]` that stops is a stop |
| Effort | the G/W rubric | from the changeset's complexity score: **1** `low` · **2–3** `medium` · **4** `high` · **5** `xhigh`; `[W]` takes its raised score (*Sizing every subagent*); `[F]` inherits `[W]`'s. Passed as `--effort`, announced and recorded |
| `[F]` continues `[W]` | `SendMessage` to `[W]`'s agent | `SendMessage` to `[W]`'s **driver**, which resumes `[W]`'s **Muse session** (`--resume <SESSION_ID>`, same name `yolo-<slug>-n<k>-W`). `[G]` is its own session, `yolo-<slug>-n<k>-G` |
| Stream | the subagent's transcript | `~/.muse-runs/<name>/stream.jsonl`; after a context reset `muse-agent.sh summary local <name>` |

**The orchestrator readies the host before the first dispatch**: `muse-agent.sh ensure local`,
guiding the user through a missing CLI or login (`muse-subagent` § *Make the host ready*), and names
`MUSE_MODEL` in the streak-map approval next to the concurrency cap. The cap still applies: the Rust
builds are the bottleneck whoever types. Announce each start as usual:

```
▶ n3 #702 [G] /green → Muse Code (<MUSE_MODEL> · high) via muse exec, driven by sonnet · low — complexity 4/5; #muse
```

Record per row in the ledger: the run name, the Muse `SESSION_ID`, `MUSE_MODEL` and the effort. A
driver's `RUN_EXIT=16` (machine stop or reboot) is resumed by the driver itself and never counts
against the round cap. A `🔴 stopped` node keeps its worktree.

**`#glm` / `#cursor-subagent`** (alias `#composer`) run this same mode through the
**`cursor-subagent` skill** instead (`.agents/skills/cursor-subagent/SKILL.md`): the Cursor agent CLI
(`agent -p`, one session per stage name continued with `--resume`) does every `[G]`, `[W]` and `[F]`,
on `glm-5p3-flash` for `#glm` or `COMPOSER_MODEL` for `#cursor-subagent`, every stage, every node.
Read the table above with `cursor-agent.sh` for `muse-agent.sh`, `~/.composer-runs/` for
`~/.muse-runs/`, and the tag's model for `MUSE_MODEL`; the Effort row does not apply (the Cursor
agent takes no effort flag). Announce each start as:

```
▶ n3 #702 [G] /green → glm-5p3-flash via cursor agent -p, driven by sonnet · low — complexity 4/5; #glm
```

### 3 — Free the per-node worktree

The per-node worktree is freed once `[W]` reports done and its head is pushed, and **always** before
the serial fixer needs that branch. The node itself is `done` when the fixer ends it with the green
condition above and `F ✅`. From the **orchestrator**:

```bash
git -C "$W" status --porcelain | grep -v '^?? '                        # must be empty — untracked build artifacts are expected
git -C "$W" rev-list --count "origin/feature/<slug>/<node>..HEAD"      # must be 0 — nothing unpushed
git worktree remove --force "$W" && git worktree prune
```

If anything is uncommitted or unpushed, do not remove: push it, or stop and ask. **Never delete the
branch** — it is the PR, and a stacked one is its successors' base. Update the ledger row, then run the
dispatch again. The fixer's worktree is removed at the end, under the same two checks.

A **stopped** node keeps its worktree: it holds the diagnosis context and the next human decision
may be to resume there. Say where it is.

## The completion gate, with pipelines running

`/plan-streak` Step 8 restores `$ORIGINAL_BRANCH` as usual — that is Hook 1 for the last node. Step 9
(a cascade `/pr-stack-rebase` per stack) needs **every branch of that stack free**, and a running
pipeline pins its own. So:

1. Restore `$ORIGINAL_BRANCH` (Step 8) and run Step 8b's links and wave-order check. Report pass 2 as
   finished **and the pipelines as running**. A Step 8b reorder or re-placement while pipelines run on
   the affected branches is a stop: it rebuilds branches they have checked out. Wait for them to drain
   first, then fix it.
2. **Wait for the pipelines to drain** — every ledger row `done`, `stopped`, `blocked` or `manual`,
   and the serial fixer finished (its worktree removed).
   The orchestrator is notified on each background subagent's completion; it does not poll in a loop.
3. Run Step 9's cascade **per stack, from the lowest layer that is actually stale**, over whatever has
   no pinned branch. A layer is stale when it does not contain its predecessor's current head. Layers
   that are `done` and current are skipped, never rewritten (see *Keep the ready lineage clean*). Do
   not cascade onto a moved trunk unless a layer conflicts with it or the user asks. Greens moved the
   stacks, so some layers above the last green will be stale; that is expected. A layer whose rebase
   changes its tests or build state is a finding. Report each layer, including the ones skipped as
   current. Standalone PRs are not cascaded.
4. A `stopped` node keeps its worktree and therefore its branch pinned: its stack's cascade stops **at
   that layer** and reports it. Do not `--force` around it. Other stacks are cascaded as usual.
5. **A rebased layer is not done until its new head is green.** Any layer whose head the cascade
   changed — or whose base moved after its own `[F]` finished, because a predecessor pushed during
   `[W]` or `[F]` — is **reopened**: set its row back to `F`, re-create its worktree (step 0), and run
   `[F]` again (the serial fixer, or a fresh one if it is gone), announced like any start, until the
   green condition holds on the new head. Cascade first, then reopen, bottom-up, so a layer is not
   re-run twice. The final report lists every reopened layer.
6. **Sweep.** With every node green, run *The sweep node*: collect per unit, plan, run the pipelines,
   then the TODO gate. The command is not finished until the gate has an answer for every open TODO. A
   `continue` answer adds nodes, which return to this gate when they are green.

## The sweep node — no new code issue leaves this streak

Planning, implementing and wrapping a streak leaves debris: `packages/*/docs/code-issues/` records that
`/pr-wrap` added or reopened, stack-shared files it declined to split (above), and entries under
`docs/dev/todo/`. `/yolo-streak` ends by paying all of it down. **No code issue the streak added or
reopened may exit the streak.** A TODO may, but only by the user's decision.

A fix can only land on a branch that contains the code it fixes, so the sweep follows the units:

- **Each stack that leaves debris gets one sweep node at its tail**, which contains every node of that
  stack.
- **A standalone node** has no tail to put one on. Its `[W]` already decomposed its files and solved
  the code issues it added, with no stack-shared exception; a code issue the collect step still finds
  on it means that `[W]` missed one — the node is **reopened at `[W]`** on its own branch (the user's
  go-ahead first, since it is `done` and frozen), and then `[F]` again.
- A streak with no stack, or whose stacks left nothing, has no sweep node; the TODO gate still runs.

Each sweep node's scope cannot be known in pass 1, so it is **reserved** at the streak-map approval
and **planned after the pipelines drain** (completion gate step 6, after the cascades).

### Collect

From the orchestrator, once every other ledger row is `done`, **per unit**:

```bash
# a stack: against its top node
mb="$(git merge-base origin/master "origin/feature/<slug>/<top-node>")"
git diff --name-status "$mb".."origin/feature/<slug>/<top-node>" -- docs/dev/todo/ 'packages/*/docs/code-issues/'
# a standalone node: the same diff against its own branch
```

Then run `/analyze-code-issues` on every package the streak touched and diff its findings against the
records at each unit's merge base: a finding that is not in the base records, or whose measurement got
worse, is **added or reopened by this streak** even if no `/pr-wrap` recorded it — and it belongs to the
unit whose diff introduced it. Pre-existing records the streak did not touch are out of scope. The
per-unit lists are the sweep backlog; put them in the ledger.

If any node is `stopped`, `blocked` or `manual`, the backlog of its unit is incomplete. Say so and ask
the user whether to sweep what exists now or wait; never sweep silently over a partial stack.

### Plan

Per stack with a backlog: a tail node via `/add-to-pr-stack`, `Type: Refactor` (`refactor(...)`
title, the streak's `(#<slug> K/N)` group with `N` raised for the whole streak), planned like any
Refactor node through the `code-restructuring` skill: changeset (with its `Complexity` score — scored
from the backlog's size, since it sizes the sweep's own implementer) with a recorded green baseline of
the suites covering the touched code (green before and after), `## Placement` naming its stack and
tail position, `## Dependencies` naming every other node of that stack as inherited, a `plan.jsonl`
per `code-restructuring`, discovery, **no PRD**. The changeset lists each backlog item by record path
and the intent that closes it. It also takes every stack-shared file `[W]` declined to split — safe
now, since nothing downstream shares it. 6e is the user's review, as for any node; the contract commit
is the plan and baseline. Re-register the stack with the sweep node on top (`gh stack link`, the full
list), and add it to the work queue when there is one.

A backlog item that **no restructure can close** is not forced into the plan: a TODO that is really
deferred feature work, a behaviour change, or something blocked on a named external. It goes to the
**TODO gate** below instead. A code issue is never one of those; if a code issue seems to need new
behaviour, split that behaviour out as its own TODO and still fix the issue.

### Run

The ordinary pipeline: `[G]` `/code-restructuring`, `[W]` `/validate-changes` + `/pr-wrap`, then the
serial `[F]` `/fix-pr` fixer, with the model rubric and consent above. Two additions:

- `[W]` re-measures with `/analyze-code-issues`. A sweep node must add **no** code issue and must close
  **every** code issue in its backlog (record deleted by the commit that resolves it, per
  `deferred-work`, with the final measurement recorded first). Anything left over is a 🔴 stop, not a
  TODO. Splitting a file is not an escape: the sweep node is where stack-shared files get split.
- Resolved TODOs are deleted by the commit that resolves them. TODOs a sweep node itself would add are
  not allowed; it either does the work or stops.

### TODO gate — the user decides

After every sweep node is green, collect the `docs/dev/todo/` entries **still open that the streak
added** (the same per-unit diffs as above, minus what the sweeps deleted). Present them, even when
`/pr-wrap`'s consent in a pipeline took its own recommendation per row. This is the one place the
streak's deferrals are shown whole:

```
### TODOs this streak leaves open
| # | TODO file | Unit | Packages | Why it was not done | Recommendation |
|---|-----------|------|----------|---------------------|----------------|
| 1 | docs/dev/todo/2026-…-….md | stack auth-store | … | … | continue the streak / defer |
```

Use `AskUserQuestion`, per row or as a group, with two answers: **continue** (add a node for it — on
top of the stack whose surface it extends via `/add-to-pr-stack`, or as a new standalone PR off the
trunk when it depends on nothing unmerged, placed by `/plan-streak`'s placement rules and planned as
the kind it really is — and run it through the pipeline, then show the gate again for whatever
remains) or **defer** (record the consent in the entry per `deferred-work`, and leave it). An empty
table is stated in one line: *"This streak leaves no open TODOs."* Never defer on the user's behalf,
and never end the command with the table unanswered.

## Consent — what invoking `/yolo-streak` pre-approves

For every node's pipeline, on that node's branch only: commit and push the green work; apply
`/pr-wrap`'s cleanups, decompositions and per-row recommendations; wrap docs, retitle and
`gh pr ready`; fix checks and review comments, post `/fix-pr`'s evidence-backed replies, and push;
remove a per-node or the fixer's worktree with `git worktree remove --force` once step 3's
tracked-changes and unpushed checks pass; `rm -rf target` between nodes in the fixer's worktree;
`git switch`, single-mode rebase onto the predecessor and push with an explicit lease inside it — the
force is for the untracked build artifacts only. Everything `green-yolo.md` lists as **not
pre-approvable** still stops that pipeline, and a stop never blocks pass 2 — only the nodes that need
the stopped one.

Also pre-approved: registering each stack with `gh stack link` as `/plan-streak` Step 7 does, appending
a sweep node per stack with `/add-to-pr-stack` and re-registering that stack, and running the sweep
pipelines. Not pre-approved: deferring a TODO (only the user's answer at the TODO gate records one),
adding a node for a TODO without that answer, reopening a `done` standalone node, and a 6h revision
that joins two units or moves a node out of a registered stack.

Not pre-approved, so not done: `#automerge`, `#forcemerge`, `#forcebuild`, `gh stack unstack`,
`--force` on a push (an explicit `--force-with-lease` with the expected SHA is pre-approved), on
`git worktree add` or on a rebase (the one exception is `git worktree remove --force` in step 3, which
*is* pre-approved; `/pr-stack-rebase`'s own `--force-with-lease` is part of that command), a re-run of
a flaky check, dispatching workflows on a branch to get a PR built, editing another node's branch, and
the cascade over pinned branches.

## Subagent report format

```
## <stage> — n<k> #<PR> (<stack name> | standalone): ✅ done | 🔴 stopped
head: <sha> (pushed: yes/no)
did: <≤5 bullets>
verified: <the scoped commands run and their result>
stopped because: <only when 🔴 — the exact command and its output, and which consent line ran out>
files left for a human: <stack-shared files not split, TODOs left, backlog delta (top node), anything else>
```

The orchestrator reads only this, so a stage that finishes with "it seems to work" has not finished.

## Final report

On top of `/plan-streak` Step 10, add the **pipeline table**, one row per node, grouped by unit:

```
fixer    [F] sonnet·med (yolo-<slug>-fix)   worktree removed
stack auth-store
  n1 #700  ✅ done      G haiku·low  W sonnet·low  (2 fix rounds)  per-node worktree freed
  n2 #701  ✅ done      G sonnet·low W sonnet·med  (1 fix round)   per-node worktree freed
  n3 #702  ✅ done      G haiku·med  W sonnet·low  (1 fix round)   per-node worktree freed
  n4 #703  ✅ done      G haiku·high W sonnet·med  (2 fix rounds)  per-node worktree freed
  n9 #709  ✅ done      sweep: 3 code issues closed, 1 TODO closed
stack web-login
  n5 #705  🔴 stopped:F G sonnet·med W opus·low  — F: third push failing `test`   fixer worktree kept at <path>
  n6 #706  ⏸ blocked:n5 (needs n5's behaviour)      never started
standalone
  n7 #707  ✅ done      G haiku·low  W sonnet·low  (1 fix round)   per-node worktree freed
  n8 #708  ✅ done      G haiku·low  W sonnet·low  (no fix round)  per-node worktree freed
```

Then: every 🔴 with its stop reason, every **blocked** or **manual** node and what unblocks it, every
link a pass-2 revision or a green moved, the stack-shared files `[W]` declined to split, and the
cascade verdict per stack and per layer. Each **sweep node** has its own row, then the backlog it took
(code issues closed, TODOs closed), and the TODO gate's table with the user's answer per row. A node
that is not ✅ is stated plainly — no pipeline is "mostly done".

## Rules

- **Run `/plan-streak` whole.** This command adds a mode; it removes nothing. 6e still gates every
  node, 6h still revises the links, and a pipeline starts only after its node's contract commit is
  pushed.
- **One streak, several units.** Stacks and standalone PRs run through the same pipeline; only a
  stacked node has lineage to keep. Waves and readiness come from the dependency graph — the behaviour
  needs — never from a node's position in a line.
- **A worktree is created only after the planning worktree has left that branch** — never before the
  next node's 6a or Step 8's restore.
- **A node starts only when every node it needs has delivered `/green`; one dependency still in `[G]`
  holds it back.** Wave 1 first (standalone nodes included); later waves as their needs' `[G]` stages
  report, not when their contract commits land.
- **Stages run in order: G → W → F.** `[G]` and `[W]` are separate fresh per-node subagents; `[F]` is
  **one long-lived serial agent** in one reusable worktree that takes the first eligible node in streak
  order. A node's next stage starts on the previous one's report, never in parallel with it. The
  orchestrator chains them; subagents do not spawn the next, and a stage agent never fans out into
  helpers.
- **Progress notes every ~50 minutes** from every stage agent to `main`; a quiet agent is checked and
  resumed.
- **Model and effort by the changeset's complexity score, always both.** On Claude Code `[G]` runs
  Haiku (low → xhigh) up to Sonnet · medium, `[W]` Sonnet · low up to Opus · medium, and the serial
  `[F]` fixer is sized once by the highest `[W]` row it will walk; on Cursor every stage uses its
  default model; under `#muse` every stage is done by Muse Code, and under `#glm` / `#cursor-subagent` by the
  Cursor agent, driven by a Sonnet · low subagent (§ *Muse mode*). **Announce one line to the user at each subagent start** (node, stage, model ·
  effort, why) and record it in the ledger.
- **Resume reads evidence, not memory.** A streak that is partly done restarts each node at its first
  unfinished stage; nothing done is redone, nothing unseen is trusted, and the status table shows the
  evidence behind every row.
- **Bind by prompt, verify by git.** A subagent confirms its toplevel and branch before writing, and
  the orchestrator confirms the head SHA is pushed before it starts the next stage.
- **Scoped local gates in every stage.** No subagent runs a workspace build, a bare `./test` or a full
  Cypress run; whole-streak health is CI's.
- **Lane caps `[G]` 1, serial `[F]` 1** by default, stated at the streak-map approval.
- **A stop is local.** It halts that node and blocks only the nodes that need it — above it in its
  stack, or with a behaviour need on it. It keeps its worktree. Pass 2, other stacks, standalone nodes
  and every unrelated pipeline carry on.
- **Green means the PR's own check rollup, on its head.** Never mark a node `done` from `gh run list`
  or a `workflow_dispatch` run. Re-run any cancelled `pull_request` run on the head after a push.
- **A done node is frozen.** Fixes go into the open node that exposed them, or into a sweep, never
  down into a ready PR. Cascades start at a stack's lowest stale layer, never at a done one, and never
  onto a moved trunk without a conflict or the user's ask.
- **Remove a worktree only when its PR is green and nothing is unpushed.** Never delete a branch.
- **No cascade, no `gh stack sync` / `rebase` from a pipeline worktree**, and no merge from anywhere
  in this command.
- **Reserve a sweep node per stack, plan it after the pipelines drain.** No code issue the streak added
  or reopened exits it; each sweep node closes its stack's issues or stops 🔴, and a standalone node's
  leftovers reopen that node. Open TODOs are shown to the user and each one is *continue* or *defer* —
  never silently left, never deferred for them.
- Never use `--no-verify`. Never amend.

## Related

**Commands**: `/plan-streak` (the whole of planning and both passes; scores each changeset's
`Complexity`), `/green`, `/validate-changes`, `/pr-wrap`, `/fix-pr`, `/green-yolo` (the single-branch
version whose consent table this reuses), `/pr-stack-rebase`, `/add-to-pr-stack`, `/repoint`,
`/merge-pr-stack`
**Skills**: `orchestrate-work` (the general queue model — stack, siblings or DAG — this command
applies), `code-restructuring`, `analyze-code-issues`, `muse-subagent` (Muse mode),
`cursor-subagent` (`#glm`, `#cursor-subagent`), `behaviour-modifiers` (`#muse`, `#glm`), `deferred-work` (record formats, the bar for deferring), `pr-stack`
(`.agents/skills/pr-stack/SKILL.md` — the model for each stack in the streak) — § *`## Green wave`*,
§ *The backlog delta a stack leaves*
