---
description: /plan-pr-stack with an implementation pipeline bolted onto wave 2 - as soon as a node's contract commit is pushed and its green-wave needs are met, give it an ephemeral worktree and run /green, then /validate-changes + /pr-wrap, then a /fix-pr loop, each in its own subagent sized to the node, and remove the worktree when the PR is green. A final **sweep node** is planned at the tail once the pipelines drain: it solves every code issue the stack added and every TODO it can, and the TODOs still open are put to the user. Planning (interview, wave 1, the 6e acceptance-test review) stays interactive. Pointed at a stack that is already partly done, it **resumes**: it reads each node's implementation status from git, GitHub and CI and starts every node at the stage it has not finished. Every subagent is started with an explicit model **and** effort taken from the changeset's complexity score, and the orchestrator tells the user which and why before each one starts
---
## YOLO PR Stack — `/plan-pr-stack`, and every ready node implemented while wave 2 keeps going

`/yolo-pr-stack` is `/plan-pr-stack` with one extra mode in **wave 2**. Run
`.agents/commands/plan-pr-stack.md` end to end — interview, decomposition approval, wave 1, wave 2's
per-node 6e acceptance-test review, the completion gate — and add the **pipeline** below at the two
points named in *Hooks into wave 2*. Nothing in `/plan-pr-stack` is skipped, shortened or relaxed. The
pipeline only starts work that wave 2 would otherwise leave for the hand-off in its Step 10.

```
wave 2 lands PRₖ (contract commit pushed) → moves on to PRₖ₊₁
        └─ PRₖ ready? ──► ephemeral worktree ──► [G] /green ──► [W] /validate-changes + /pr-wrap ──► [F] /fix-pr loop ──► worktree removed
                                                   [G] and [W]+[F] = background subagents; [F] continues [W]'s agent, model sized to the node
```

```
pipelines drain → cascade → collect the stack's open TODOs + code issues → plan the SWEEP node (tail) → its pipeline
                                                                              └─ TODOs still open ──► user: continue the stack | defer
```

The orchestrator is the session running wave 2. It owns the worktrees, picks each subagent's model,
starts the next stage when the previous one reports, and keeps the interactive parts interactive.
Subagents cannot ask the user anything; each one runs under **pre-approved consent** (below) and
stops with a 🔴 report where that consent runs out.

## Resume mode — a stack that is already partly done

Run this command at a stack that already exists — the user gives a PR in the stack, the stack slug,
or says "resume" while a stack branch is checked out — and it does **not** re-plan. It reads where
every node actually stands and starts each node at the first stage it has not finished. Nothing that
is already done is redone, and nothing is trusted that cannot be seen.

**Evidence, not memory.** A ledger file in the scratchpad is a *hint* at what happened; git, GitHub and
CI are the truth. If they disagree, the evidence wins and the final report names the disagreement.

### Step R1 — locate the stack and its edges

`gh stack view` for topology; each node's changeset `## Green wave` for dependencies (the same source
Dispatch uses); the changeset at each branch head
(`git show origin/feature/<slug>/<node>:docs/dev/1-WIP/<file>`, or the wrapped docs if it is already
gone from `1-WIP/`). A branch with no registered stack is not a stack: stop and say so.

### Step R2 — derive each node's status

Read-only, from the orchestrator, one node at a time. **First row that matches wins:**

| Evidence on the node | Status | Starts at |
|---|---|---|
| PR merged | `landed` | nothing — counts as delivered. If a successor is still based on its branch, say `/repoint` is needed; this command does not do it |
| PR ready (not draft), changeset gone from `1-WIP/` (wrapped), every check terminal-green and every thread addressed **on the current head SHA** | `done` | nothing |
| PR ready and wrapped, but a check is red or running, or a thread is open | `F` | `[F]` |
| Tests green and pushed, PR still draft or its changeset still in `1-WIP/` | `W` | `[W]` |
| Contract commit pushed, commits after it, tests **still red** | `G:resume` | `[G]`, told what exists |
| Contract commit pushed, nothing after it | `G` | `[G]` |
| Plan commit pushed, no contract commit | `plan:wave2` | wave 2 for this node (6a–6g, **6e stays interactive**) |
| No branch or PR yet | `plan:wave1` | `/plan-pr-stack` from the step that creates it |
| Contract commit carries no failing test (see *Which nodes qualify*) | `manual` | left for the Step 10 hand-off, as in Dispatch |

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

Print one table: node, PR, status, the evidence that decided it (a SHA, a check name, a thread count),
the complexity score, and the stage it starts at. Then run Dispatch as normal, with these changes:

- A node's **ledger row begins at its derived stage**, not at `queued`. `G:resume`, `W` and `F` rows
  start at that stage's subagent; a node ahead of the others never waits for them.
- A dependency counts as **delivered** when it is `W`, `F`, `done` or `landed` — its `[G]` happened.
- Each stage brief for a resumed node adds the facts the stage needs and says: **re-verify, do not
  redo.** `G:resume` gets the head SHA, the commits after the contract commit and the tests still
  failing, and builds on them. `W` re-runs its own validations rather than trusting that green was
  proven. An `[F]` with no `[W]` agent to continue is a **fresh** agent, sized by `[W]`'s row, that
  first reads the diff and the changeset.
- **Nodes with `plan:` statuses are planned first**, interactively, because wave 2 is bottom-up and
  their acceptance tests need the user. The implementation pipelines of nodes already past their
  contract commit start alongside, as in the ordinary command — planning never waits on them and
  they never wait on it.
- If the stack already has its **sweep node**, it is an ordinary node with these statuses. If not, it is
  reserved and planned at the tail after the pipelines drain, as usual. The TODO gate shows only TODOs
  with no recorded consent; an entry that already carries a `defer` consent line is not asked again.
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
- **`[F]` continues `[W]`'s agent**, so it keeps `[W]`'s model and effort. A fresh `[F]` (the original
  is gone — context exhausted, restart, or a reopened layer) uses `[W]`'s row for the node.

### Announce the choice — every time a subagent starts

Before each Agent call, in the orchestrator's own text, **one line to the user** saying what was chosen
and why. One line per subagent, never batched, never skipped because several nodes dispatch at once.
It names the node, the stage, the model · effort, and the **reason** — the complexity score and the
signals behind it:

```
▶ n3 #702 [G] /green → haiku · xhigh — complexity 4/5 (two crates, proto change); first attempt
▶ n1 #700 [W] /validate-changes + /pr-wrap → sonnet · medium — complexity 2/5 raised by 1: [G] needed its retry
↗ n3 #702 [G] retry → sonnet · medium — haiku · xhigh stopped 🔴 out of its depth on the proto seam
▶ n3 #702 [F] /fix-pr → continues [W]'s agent (sonnet · medium, effort inherited) — round 1
▶ n5 #705 [G] /green → haiku · high — complexity 3/5 moved up one row: contract commit has 14 failing tests, plan said 6
▶ n2 #701 [W] /validate-changes + /pr-wrap → sonnet · low — complexity 1/5; resuming at W, evidence: scoped probe green on 3fa9c1e
```

The same model, effort and reason go in the ledger row. A user who disagrees can say so on the spot;
the next start uses their answer.

## What stays interactive

Planning is the user's call and this command does not change that: the interview, the decomposition
approval, and **6e — the per-node acceptance-test review**. A node's contract commit is pushed only
after its 6e approval, so a pipeline never starts on tests the user has not seen.

## Hooks into wave 2

**Hook 1 — after 6g, once the planning worktree has left PRₖ.** The branch is pinned only while it
is checked out here, so the order is: finish 6g for PRₖ → `git checkout` PRₖ₊₁ (its 6a), or, for the
last node, Step 7's restore of `$ORIGINAL_BRANCH` → *then* run the **dispatch** below. Never create
the worktree for PRₖ while this worktree still has it checked out; `git worktree add` refuses, and
the workaround (`--force`, detaching) is the pinning problem `/plan-pr-stack` 6a exists to avoid.

**Hook 2 — whenever a pipeline's `[G]` stage reports green.** That can unblock nodes whose contract
commit is already pushed. Run the dispatch again. It also re-runs after every pipeline finishes, to
refill the concurrency cap.

**A green pushed by a pipeline moves the stack under wave 2.** 6b already rebases each node onto its
parent's tip before its contract commit, so a node planned after its parent went green simply picks
the green up. That is expected, not a leak — 6b's *exactly ONE commit* check counts commits above the
parent's **current** tip, whatever that tip now holds.

### Dispatch — which nodes start

A node is **ready** when all of these hold:

1. Its contract commit (commit 2) is pushed and its 6e was approved.
2. It has no **started** pipeline: its ledger row is absent, `queued` or `blocked:n<j>`. A row in
   `G`, `W`, `F`, `done` or `stopped` is excluded. (In resume mode a row is created at the node's
   derived stage — see *Step R3*.) `queued` and `blocked` rows are **re-evaluated by
   every dispatch** and move to `G` the moment rules 1 and 3–5 hold, which is how a node waiting for a
   free slot or a resumed blocker ever starts.
3. **Every node it depends on has delivered `/green`.** The dependencies are the real edges its
   changeset's `## Green wave` lists — the committed source, current after 6g (6f can add or drop
   one). Branch-line neighbours that are not dependencies do not count. A dependency has delivered
   when its `[G]` stage reported ✅ with the head pushed. **While any dependency's `[G]` is queued, in
   progress, or being retried, this node waits, even with a free slot and a pushed contract commit.**
   Its worktree is not created, and no subagent starts. Wave 1 nodes have no dependencies. `/green`
   would stop at its dependency gate (`green.md` 0b) anyway, so an early start is only a wasted
   worktree.
4. No node it needs is in a 🔴 stopped state. Mark it **blocked**, not failed, and say which node
   blocks it.
5. The running count is below the **concurrency cap**.

**Concurrency cap: 3**, unless the user set another number in this request. Each worktree builds its
own Rust `target/` (several GB per worktree, more with `tddy-web` deps), every green PR that lands
makes the layers above it stale (`plan-pr-stack` Step 9 § *What concurrency costs*), and parallel
cargo builds compete for the machine. Say the cap in the Step 1 decomposition approval so the user can
change it before anything is created. Say there too that a **sweep node** will be appended at the tail
(see *The sweep node*), with its scope unknown until the stack has been implemented. Nodes that are
ready beyond the cap queue in stack order, blockers first.

### Which nodes qualify

**Only nodes whose contract commit has something red qualify for this pipeline.** `/green` stops when
no test is red, and a node whose commit 2 is a recorded baseline (a mechanical move, toolchain or CI
change, a test rebalance whose characterisation tests are already green) has none. So: nodes whose
contract commit carries **failing tests** run the pipeline, and **Refactor** nodes (`Type: Refactor`,
planned through `code-restructuring` with a `plan.jsonl`) run it with `/code-restructuring` as their
`[G]` implementer. Every other node gets ledger status `manual` and is left for the `/plan-pr-stack`
Step 10 hand-off. Its dependents wait as `blocked:n<j>` until a human finishes the node, and the final
report says so. A changeset that names a different implementer wins.

### Ledger

Keep one file in the scratchpad: `yolo-pr-stack-<slug>.md`, one row per node — branch, PR number,
worktree path, stage (`queued · G · W · F · done · stopped:<stage> · blocked:n<j> · manual`), the
complexity score, **model · effort used per stage with the reason** (both attempts, when retried),
head SHA at the last report. Update it on every report. It is the orchestrator's memory across a
context reset: after one, re-derive stages from `git worktree list`, the branches' pushed state and
each PR's checks, never restart a node whose row is past `queued`/`blocked`. The ledger is not
committed.

## The pipeline, per ready node

### 0 — Ephemeral worktree

```bash
W="$(dirname "$PWD")/yolo-<slug>-n<k>"                   # a sibling of the planning worktree
git fetch origin "feature/<slug>/<node>"
git worktree add "$W" "feature/<slug>/<node>"            # must succeed: nothing pins the branch
git -C "$W" status --porcelain | grep -v '^?? '          # must be empty
git -C "$W" rev-list --count "origin/feature/<slug>/<node>..HEAD"   # must be 0
(cd "$W" && ./dev cargo build -p <pkg>)                  # each package in scope; never a workspace build
(cd "$W" && ./dev bun run local-registry-install)        # only if the node touches packages/tddy-web
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
- **Scoped gates only** — `./test -p <pkg>`, `cargo clippy -p <pkg> -- -D warnings`, the single spec
  under change; never a bare `./test` or a full Cypress run
  ([AGENTS.md § Verification](../../AGENTS.md#verification-scope-it-locally-run-it-in-full-on-ci)).
- The branch is a stack member and a successor's base: **commit and push** before reporting.
- Report in the format at the end of this file, and nothing else.

Fork-style context is not needed and costs tokens. Use a **fresh** subagent for `[G]` and a **fresh**
one for `[W]` (stage `[F]` continues `[W]`'s agent, see below) with the brief above, the PR number,
the changeset path, and the command to run. Name the stage's consent section below; do not paste the
whole of this file.

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
sibling pipelines have those branches pinned. So `pr-wrap` step 3.5's own stack-branch rule applies:
**do not split a stack-shared file**; record it as `pr-wrap` says and list it in the report. The sweep
node splits it later. Every other file is decomposed now, as `green-yolo` requires.

**One deliberate difference from `/pr-wrap`: the top node's backlog sweep.** `pr-wrap` step 8 asks the
top node to judge the stack's `docs/dev/todo/` delta and offer *Extra node* verdicts. Under this
command that question belongs to *The sweep node* and its TODO gate, so `[W]` lists the delta in its
report and does not ask.

**Model and effort.** `[W]`'s row for the node's complexity, raised by one when the *Sizing* rule says
so — pass both `model` and `effort`, announce first.

### Stage F — `/fix-pr`, looped

Start only when `[W]` reported done. **Continue `[W]`'s own agent** with `SendMessage` (load it with
`ToolSearch("select:SendMessage")` if its schema is not loaded) — do not spawn a new one. Same context,
same conversation: it already holds the diff, the changeset and every cleanup decision, which is what
makes its answers to review comments accurate, and it is already bound to `<W>`. The message names
the command and the stop conditions below. Cursor: resume the same subagent the same way. Run
`.agents/commands/fix-pr.md` in `<W>`, **repeatedly**, until both hold on the **current head SHA**:

- every check is terminal and passing (`scripts/ci-status.sh <N>`; readying the PR in `[W]` is what
  started its build), and
- every review thread is addressed — the main (non-nit) comments fixed or answered with evidence, per
  `fix-pr.md`'s reaction protocol.

One `/fix-pr` run already re-checks running checks and fixes failures as they appear, so the loop is
for what it hands back: a new push that restarts CI, a new comment. Between rounds the subagent
re-reads the PR state; it does not sleep-poll. Consent is `green-yolo.md` § *Phase C* — including its
stops, which end the loop with a 🔴 and **never** re-run a flaky check unprompted:

- a flaky or infra failure that needs a re-run → stop, ask;
- the same check failing on a **third** consecutive push → stop;
- a fix for one failure that keeps causing another → stop;
- **a round cap of 5.** A fifth round with the PR still not green is a stop, not a sixth try.

**This command does not merge.** That is `/green-yolo` Phase D, and nobody invoked it here. Landing
stays with `/merge-pr-stack`.

**Model and effort.** Whatever `[W]`'s agent already runs — a continued agent cannot change either. So
`[F]` has `[W]`'s model and effort, and there is no escalation or de-escalation; the announcement says
`continues [W]'s agent` with those values. A check that keeps failing or a thread arguing a design
point is what the round cap and the stops are for: the stage stops and reports. If the agent is no
longer resumable (context exhausted, harness restarted), start a fresh agent on `[W]`'s row for the
node, with the same brief, announce it, and say so in the ledger.

### Models on Cursor

The Haiku / Sonnet / Opus tables above are the **Claude Code** choice. On **Cursor**, every stage runs
on Cursor's default subagent model for every node whatever its size — Cursor has no per-node model
ladder to size against, so the complexity rubric does not choose a model or effort there. Still
announce each start, with the complexity score and `effort n/a`. Binding to the worktree, the stage
order, the consent and the stops are identical on both harnesses.

### 3 — Green: remove the worktree

The node is done when stage F ends with the green condition above. Then, from the **orchestrator**:

```bash
git -C "$W" status --porcelain | grep -v '^?? '                        # must be empty — untracked build artifacts are expected
git -C "$W" rev-list --count "origin/feature/<slug>/<node>..HEAD"      # must be 0 — nothing unpushed
git worktree remove --force "$W" && git worktree prune
```

If anything is uncommitted or unpushed, do not remove: push it, or stop and ask. **Never delete the
branch** — it is the PR, and successors are based on it. Mark the ledger row `done`, then run the
dispatch again.

A **stopped** node keeps its worktree: it holds the diagnosis context and the next human decision
may be to resume there. Say where it is.

## The completion gate, with pipelines running

`/plan-pr-stack` Step 7 restores `$ORIGINAL_BRANCH` as usual — that is Hook 1 for the last node. Step 8
(the cascade `/pr-stack-rebase from #1 to #n`) needs **every stack branch free**, and a running
pipeline pins its own. So:

1. Restore `$ORIGINAL_BRANCH` (Step 7) and run Step 7b's wave-order check. Report wave 2 as finished
   **and the pipelines as running**. A Step 7b reorder while pipelines run is a stop: it rebuilds
   branches they have checked out. Wait for them to drain first, then reorder.
2. **Wait for the pipelines to drain** — every ledger row `done`, `stopped`, `blocked` or `manual`.
   The orchestrator is notified on each background subagent's completion; it does not poll in a loop.
3. Run Step 8's cascade over whatever has no pinned branch. Greens moved the stack, so *"every layer
   already current"* is **not** the expected answer here, and a rebased layer is not a finding to
   alarm on — but a layer whose rebase changes its tests or build state is. Report each layer.
4. A `stopped` node keeps its worktree and therefore its branch pinned: the cascade stops **at that
   layer** and reports it. Do not `--force` around it.
5. **A rebased layer is not done until its new head is green.** Any layer whose head the cascade
   changed — or whose base moved after its own `[F]` finished, because a predecessor pushed during
   `[W]` or `[F]` — is **reopened**: set its row back to `F`, re-create its worktree (step 0), and run
   `[F]` again with a fresh agent on `[W]`'s row for that node (the original one is gone with its
   worktree), announced like any start, until the green condition holds on the new head, then remove
   the worktree again. Cascade first, then reopen, bottom-up, so a layer is not re-run twice. The
   final report lists every reopened layer.
6. **Sweep.** With every layer green, run *The sweep node*: collect, plan, run its pipeline, then the
   TODO gate. The command is not finished until the gate has an answer for every open TODO. A
   `continue` answer adds nodes, which return to this gate when they are green.

## The sweep node — no new code issue leaves this stack

Planning, implementing and wrapping a stack leaves debris: `packages/*/docs/code-issues/` records that
`/pr-wrap` added or reopened, stack-shared files it declined to split (above), and entries under
`docs/dev/todo/`. `/yolo-pr-stack` ends by planning **one extra node at the tail** whose job is to pay
all of it down. **No code issue the stack added or reopened may exit the stack.** A TODO may, but only
by the user's decision.

The node's scope cannot be known in wave 1, so it is **reserved** at the decomposition approval and
**planned after the pipelines drain** (completion gate step 6, after the cascade).

### Collect

From the orchestrator, against the stack's tail, once every other ledger row is `done`:

```bash
mb="$(git merge-base origin/master "origin/feature/<slug>/<top-node>")"
git diff --name-status "$mb".."origin/feature/<slug>/<top-node>" -- docs/dev/todo/ 'packages/*/docs/code-issues/'
```

Then run `/analyze-code-issues` on every package the stack touched and diff its findings against the
records at `$mb`: a finding that is not in the base records, or whose measurement got worse, is
**added or reopened by this stack** even if no `/pr-wrap` recorded it. Pre-existing records the stack
did not touch are out of scope. The list is the sweep's backlog; put it in the ledger.

If any node is `stopped`, `blocked` or `manual`, the backlog is incomplete. Say so and ask the user
whether to sweep what exists now or wait; never sweep silently over a partial stack.

### Plan

A tail node via `/add-to-pr-stack`, `Type: Refactor` (`refactor(...)` title), planned like any
Refactor node through the `code-restructuring` skill: changeset (with its `Complexity` score — scored
from the backlog's size, since it sizes the sweep's own implementer) with a recorded green baseline of
the suites covering the touched code (green before and after), `## Dependencies` naming every other
node as inherited, a `plan.jsonl` per `code-restructuring`, discovery, **no PRD**. The changeset lists
each backlog item by record path and the intent that closes it. It also takes every stack-shared file
`[W]` declined to split — safe now, since nothing downstream shares it. 6e is the user's review, as for
any node; the contract commit is the plan and baseline.

A backlog item that **no restructure can close** is not forced into the plan: a TODO that is really
deferred feature work, a behaviour change, or something blocked on a named external. It goes to the
**TODO gate** below instead. A code issue is never one of those; if a code issue seems to need new
behaviour, split that behaviour out as its own TODO and still fix the issue.

### Run

The ordinary pipeline: `[G]` `/code-restructuring`, `[W]` `/validate-changes` + `/pr-wrap`, `[F]`
`/fix-pr`, with the model rubric and consent above. Two additions:

- `[W]` re-measures with `/analyze-code-issues`. The sweep node must add **no** code issue and must
  close **every** code issue in its backlog (record deleted by the commit that resolves it, per
  `deferred-work`, with the final measurement recorded first). Anything left over is a 🔴 stop, not a
  TODO. Splitting a file is not an escape: the sweep node is where stack-shared files get split.
- Resolved TODOs are deleted by the commit that resolves them. TODOs the sweep node itself would
  add are not allowed; it either does the work or stops.

### TODO gate — the user decides

After the sweep node is green, collect the `docs/dev/todo/` entries **still open that the stack added**
(the same diff as above, minus what the sweep deleted). Present them, even when `/pr-wrap`'s consent in
a pipeline took its own recommendation per row. This is the one place the stack's deferrals are shown
whole:

```
### TODOs this stack leaves open
| # | TODO file | Packages | Why it was not done | Recommendation |
|---|-----------|----------|---------------------|----------------|
| 1 | docs/dev/todo/2026-…-….md | … | … | continue the stack / defer |
```

Use `AskUserQuestion`, per row or as a group, with two answers: **continue** (add a node for it via
`/add-to-pr-stack` — plan it as the kind it really is — and run it through the pipeline, then show
the gate again for whatever remains) or **defer** (record the consent in the entry per
`deferred-work`, and leave it). An empty table is stated in one line: *"This stack leaves no open
TODOs."* Never defer on the user's behalf, and never end the command with the table unanswered.

## Consent — what invoking `/yolo-pr-stack` pre-approves

For every node's pipeline, on that node's branch only: commit and push the green work; apply
`/pr-wrap`'s cleanups, decompositions and per-row recommendations; wrap docs, retitle and
`gh pr ready`; fix checks and review comments, post `/fix-pr`'s evidence-backed replies, and push;
remove the ephemeral worktree with `git worktree remove --force` once step 3's tracked-changes and
unpushed checks pass — the force is for the untracked build artifacts only. Everything `green-yolo.md`
lists as **not pre-approvable** still stops that pipeline, and a stop never blocks wave 2 — only the
nodes that need the stopped one.

Also pre-approved: appending the sweep node with `/add-to-pr-stack` and running its pipeline. Not
pre-approved: deferring a TODO (only the user's answer at the TODO gate records one) and adding a node
for a TODO without that answer.

Not pre-approved, so not done: `#automerge`, `#forcemerge`, `#forcebuild`, `gh stack unstack`,
`--force` on a push, on `git worktree add` or on a rebase (the one exception is
`git worktree remove --force` in step 3, which *is* pre-approved; `/pr-stack-rebase`'s own
`--force-with-lease` is part of that command), a re-run of a flaky check, editing another node's
branch, and the cascade over pinned branches.

## Subagent report format

```
## <stage> — n<k> #<PR>: ✅ done | 🔴 stopped
head: <sha> (pushed: yes/no)
did: <≤5 bullets>
verified: <the scoped commands run and their result>
stopped because: <only when 🔴 — the exact command and its output, and which consent line ran out>
files left for a human: <stack-shared files not split, TODOs left, backlog delta (top node), anything else>
```

The orchestrator reads only this, so a stage that finishes with "it seems to work" has not finished.

## Final report

On top of `/plan-pr-stack` Step 9, add the **pipeline table**, one row per node:

```
n1 #700  ✅ done      G haiku·low  W+F sonnet·low (2 fix rounds)  worktree removed
n2 #701  ✅ done      G sonnet·low W+F sonnet·med (1 fix round)   worktree removed
n3 #702  🔴 stopped:F G sonnet·med W+F opus·low  — F: third push failing `test`   worktree kept at <path>
n4 #703  ⏸ blocked:n3 (needs n3's behaviour)      never started
```

Then: every 🔴 with its stop reason, every **blocked** or **manual** node and what unblocks it, the
stack-shared files `[W]` declined to split, and the cascade verdict per layer. The **sweep node** has
its own row, then the backlog it took (code issues closed, TODOs closed) and the TODO gate's table
with the user's answer per row. A node that is not ✅ is stated plainly — no pipeline is "mostly done".

## Rules

- **Run `/plan-pr-stack` whole.** This command adds a mode; it removes nothing. 6e still gates every
  node, and a pipeline starts only after its node's contract commit is pushed.
- **A worktree is created only after the planning worktree has left that branch** — never before 6g's
  checkout of the next node or Step 7's restore.
- **A node starts only when every dependency has delivered `/green`; one dependency still in `[G]`
  holds it back.** Wave 1 first; later waves as their predecessors' `[G]` stage reports, not when
  their contract commit lands.
- **Stages run in order: G → W → F.** `[G]` and `[W]` are separate fresh subagents; `[F]` **continues
  `[W]`'s agent** rather than starting another. The next stage starts on the previous one's
  report, never in parallel with it. The orchestrator chains them; subagents do not spawn the next.
- **Model and effort by the changeset's complexity score, always both.** On Claude Code `[G]` runs
  Haiku (low → xhigh) up to Sonnet · medium, `[W]` Sonnet · low up to Opus · medium, `[F]` inherits
  `[W]`'s agent; on Cursor every stage uses its default model. **Announce one line to the user at
  each subagent start** (node, stage, model · effort, why) and record it in the ledger.
- **Resume reads evidence, not memory.** A stack that is partly done restarts each node at its first
  unfinished stage; nothing done is redone, nothing unseen is trusted, and the status table shows the
  evidence behind every row.
- **Bind by prompt, verify by git.** A subagent confirms its toplevel and branch before writing, and
  the orchestrator confirms the head SHA is pushed before it starts the next stage.
- **Scoped local gates in every stage.** No subagent runs a workspace build, a bare `./test` or a full
  Cypress run; whole-stack health is CI's.
- **Concurrency cap 3** by default, stated at the decomposition approval.
- **A stop is local.** It halts that node and blocks only the nodes that need it. It keeps its
  worktree. Wave 2 and every unrelated pipeline carry on.
- **Remove a worktree only when its PR is green and nothing is unpushed.** Never delete a branch.
- **No cascade, no `gh stack sync` / `rebase` from a pipeline worktree**, and no merge from anywhere
  in this command.
- **Reserve a sweep node, plan it after the pipelines drain.** No code issue the stack added or
  reopened exits it; the sweep node closes them all or stops 🔴. Open TODOs are shown to the user and
  each one is *continue* or *defer* — never silently left, never deferred for them.
- Never use `--no-verify`. Never amend.

## Related

**Commands**: `/plan-pr-stack` (the whole of planning and both waves; scores each changeset's
`Complexity`), `/green`, `/validate-changes`, `/pr-wrap`, `/fix-pr`, `/green-yolo` (the single-branch
version whose consent table this reuses), `/pr-stack-rebase`, `/add-to-pr-stack`, `/repoint`,
`/merge-pr-stack`
**Skills**: `code-restructuring`, `analyze-code-issues`, `deferred-work` (record formats, the bar for
deferring), `pr-stack` (`.agents/skills/pr-stack/SKILL.md`) — § *`## Green wave`*, § *The backlog delta a
stack leaves*
