# PRD: PR-Stack panel — expandable rows, session links, persisted order, base sync

**Product area:** Coder (PR stacking) + Web (PR-Stack Chat Screen)
**Created:** 2026-08-01
**Status:** WIP
**Related:** [PR stacking](../pr-stacking.md), [PR-Stack live status & repoint](../pr-stack-live-status.md),
[Session drawer § PR-Stack Chat Screen](../../web/session-drawer.md#pr-stack-chat-screen)

## Summary

The **Planned PRs** panel on the PR-Stack Chat Screen is the operator's control surface for a stack: one row
per `StackNode`, offering **Start session** and **Repoint**. It tells the operator what each planned PR *is*,
but not what state it is *in* relative to the rest of the stack, and it offers no way to act on that state.

This feature adds five capabilities to the panel:

1. **Each row expands** to its full detail instead of rendering everything at once.
2. **The spawned indicator opens the child session** it is bound to.
3. **Row order is persisted** — and operator-reorderable — instead of being re-derived from the DAG.
4. **Each row states whether its branch conflicts with its base**, live, without the agent running.
5. **A row cleanly behind its base offers a one-click pull**, labelled with the commit count.

## Problem

### 1. The row shows everything, always

`PlannedPrRow` renders title, description, owned branch, planned branch, base branch and worktree path as six
unconditional lines, then a horizontal strip of badges. In the panel's 360px dock a five-node stack is a wall
of text with no visual hierarchy — and there is nowhere to put the fields a node genuinely *has* but the row
has never shown: its `node_id`, its `parents`, its `child_recipe`, its bound child session.

The row is deliberately unconditional today
([live status § D16](../pr-stack-live-status.md#repointing-a-dead-end-planned-pr-added-2026-07-26)) because an
earlier design *replaced* a blocked row's contents with a single indicator, costing the operator everything
the row knew at the moment they most needed it. **That decision is not being reversed.** Collapsing is not
suppression: every field remains in the row, one interaction away, and the states that demand attention —
blockers, refusals, status — stay in the always-visible region.

### 2. "spawned" is a dead end

Once a node is spawned, its call-to-action slot collapses to a plain `<span>` reading its phase. The node
records `session_id`, and `QueryBranch` resolves the live session that owns the branch, but neither is a link:
the operator reads "building" and then hunts for the matching row in the session drawer by eye. A test id
`pr-stack-session-<nodeId>` has been reserved for this since the live-status work and no component renders it.

### 3. Rows reshuffle under the cursor

Order is derived, never stored. The panel sorts by topological order, which is stable *given its inputs* — but
both inputs move underneath it:

- a chat-driven plan refinement re-seeds `Changeset.stack` and rewrites the whole `nodes` list;
- a merge, a repoint, or `pr_set_parents` rewrites `parents`, which re-layers the topology.

So a row the operator was reading jumps position as a consequence of an unrelated event. The backend has no
ordering primitive at all — `pr_stack/mod.rs` states plainly that order *is* the parent graph and that
re-parenting is the only way to reorder. That conflates two different things: **the dependency graph** (which
node builds on which) and **the reading order** (how the operator wants the list laid out). They are allowed
to differ, and only the first should change when the stack's shape changes.

### 4. Nothing says whether a node conflicts with its base

`has-conflicts` exists as an `internal_status` kind, but it is **never derived** — the derivation table in
`internal_status.rs` produces `merged`, `needs-repoint`, `ready-to-merge` and `up-to-date` only. A node reads
`has-conflicts` solely because an agent called `pr_resolve_conflicts` or `pr_set_status`, so in the normal case
— the orchestrator agent idle, the operator looking at the panel — the badge is absent or stale.

The one real detector, `pr_resolve_conflicts_action`, runs `git merge --no-commit --no-ff` and then
`git merge --abort`. That **mutates the index and the working tree**, so it cannot be run on a poll: the
worktree it would touch may have a child session's agent working in it.

### 5. There is no way to take base changes

When a predecessor lands commits, the node's branch falls behind. The operator's only options are to ask the
agent in chat or to open a terminal in the worktree. **Repoint is the wrong tool** — it answers "this node
belongs somewhere else now" by dropping parent edges, whereas the operator wants to stay stacked exactly where
they are and simply take what the base has.

## Capabilities

### C1 — A row collapses to a summary and expands to its detail

- **Collapsed**, a row shows its title, its status badges (in-progress, PR link and state, internal status,
  base sync) and its call-to-action slot (Start session, or the spawned indicator), plus the always-visible
  warning and error region.
- **Expanded**, it additionally shows description, owned branch or planned branch, base branch, worktree path,
  node id, parent titles, child recipe, child state, bound child session, and any conflicted paths.
- The detail is **hidden, never unmounted**, so a row keeps its expansion across a poll tick and across the
  panel being closed and reopened.
- **Blockers, refusals and errors are never inside the collapse boundary.** A reason the operator has to
  expand a row to discover is a fresh dead end of exactly the kind D16 exists to remove.

### C2 — The spawned indicator opens its bound session

Clicking it selects and attaches that session, exactly as clicking the session in the drawer does. Resolution
prefers the session the **plan** records (`StackNode.session_id`) and falls back to the session that currently
**owns the branch**; when neither resolves to a known session the indicator stays plain text rather than
offering a link that would select nothing.

An orphaned node is unaffected: it already shows **Start session** rather than a status indicator
([live status § Orphaned-node recovery](../pr-stack-live-status.md#orphaned-node-recovery-added-2026-07-26)),
so there is never a link to a session that was deleted.

### C3 — Display order is persisted and operator-editable

- `StackNode` gains a persisted display order. The panel renders strictly by it.
- **It never changes as a side effect.** A merge, a repoint, a status refresh, a re-parenting: none of them
  move a row.
- New nodes are appended at the bottom; a deleted node leaves the survivors' positions alone.
- Each row offers **move up** / **move down**, the deliberate act that does change order.
- A plan authored before this feature has no order recorded. It renders in today's topological order until the
  next write to the stack, which numbers every node — so nothing regresses and nothing needs migrating.

### C4 — Each row states its standing against its base

Resolved server-side on the existing branch poll, so it is live whether or not the orchestrator agent is
running, and reported as one of:

| State | Row shows |
|---|---|
| Behind by N commits, no conflicts | "N behind `<base>`" |
| Would conflict | a conflict badge; the conflicted paths in the expanded detail |
| Contains every commit on its base | "in sync with `<base>`" |
| The comparison could not be made | "base status unavailable", with the reason as a tooltip |
| The poll has not answered yet | nothing |

**"Could not tell" is never rendered as "clean."** A comparison that failed arrives byte-identical to a
healthy one — no commits behind, no conflicts — so it carries an explicit unavailable discriminator. This is
the same rule as [D12](../pr-stack-live-status.md#authenticated-pr-status-added-2026-07-26) for PR status, and
it exists because conflating the two is precisely how a live open PR stayed invisible for a day.

**"In sync" is a badge, not silence.** If only the bad states rendered, a healthy row and a row whose poll has
not answered would look identical — the same conflation one level down.

The comparison is **strictly non-mutating**: it never checks out, never merges, never writes a ref, and never
fetches. It reads the base as of the last fetch, which makes it conservative in the same way the existing
remote-branch probe already is — it can report a node behind when it has just caught up, never the reverse.

### C5 — A row behind its base offers to pull

- Offered **only** when the node is behind with no conflicts and owns a branch. Not at zero commits, not on
  conflicts, not when the comparison is unavailable or has not arrived.
- **Merge is the default** — it adds a merge commit, rewrites no history, and disturbs no review anchors on
  the open PR. It is what the stack's own `pr_resolve_conflicts` already does.
- **Rebase is offered beside it** as an explicit operator choice, matching what repoint does.
- **A successful pull pushes**, or the PR on GitHub keeps reporting itself behind and the row contradicts what
  a reviewer sees. A merge pushes fast-forward; a rebase force-pushes with a lease, so a concurrent push
  aborts it rather than being clobbered.
- **A conflict aborts and reports.** Nothing is left half-merged. Unlike the agent-facing
  `pr_resolve_conflicts` — which deliberately leaves conflicts in the tree because the agent is about to be
  prompted to resolve them — this is a dashboard button that may be pressed while an agent is mid-turn in that
  worktree, with nobody in scope to resolve anything.
- **A dirty worktree is a prompt, not a refusal.** The row warns before the click, and confirming offers to
  **commit and push the outstanding changes first**, then pull. Auto-stashing was rejected: a `stash pop` can
  conflict on its own and would leave a child session's checkout in a state nobody asked for.
- **A push failure is reported, not rolled back.** The local work landed; undoing it would be strictly worse
  than saying so.

## Design decisions

Continuing the numbering in [PR-Stack live status](../pr-stack-live-status.md) (which reaches D20).

| # | Decision | Rationale |
|---|---|---|
| D21 | A row's detail is **hidden, not unmounted** | Expansion, scroll position and the branch poll set all survive a collapse, matching the stance `PlannedPrPanel` already takes. It also keeps every existing information contract intact — the row still renders its full information, one interaction away, so D16 is honoured rather than reversed. |
| D22 | Blockers, refusals and errors stay **outside** the collapse boundary | These are the states that tell the operator why an action is unavailable. A reason that requires expanding a row to find is the dead end D16 removed. |
| D23 | The bound session is resolved from the **plan first**, the branch owner second | The chip is the *node's* recorded binding and the plan is the durable record; "who owns this branch right now" is a different question whose answer changes after a resume or a hand-off. Both are guarded on the session actually being known, so a link never selects nothing. |
| D24 | Display order is **persisted per node**, not derived | The dependency graph and the reading order are different facts. Deriving one from the other means every merge, repoint and re-parenting silently rewrites the operator's view. |
| D25 | A legacy stack falls back to topological order **wholesale**, not per node | A half-numbered plan has no coherent total order, and interleaving real positions with invented ones can render a child above its parent — a worse lie than one render of a correct derived order. The next write numbers everything and the fallback stops applying. |
| D26 | Base sync is computed with a **non-mutating merge probe**, and never fetches | It runs on a 5s poll against worktrees that may have a live agent in them, so the existing `git merge --no-commit` detector is disqualified. Fetching is disqualified for cost: the authoritative default-branch resolver runs `git fetch origin`, which cannot be on a poll path. |
| D27 | An unavailable comparison is **never** rendered as clean | A failed comparison is byte-identical to a healthy one. D12 restated one level down. |
| D28 | The **base being compared is reported back** and is what the row names | The counts are meaningless without the ref they came from, and the base may resolve locally rather than remotely. The row states what was compared, not what it asked for — the same discipline as [D18](../pr-stack-live-status.md#repointing-a-dead-end-planned-pr-added-2026-07-26). |
| D29 | An **unnamed base is unavailable**, not substituted | Repoint substitutes the resolved default for an empty target (D20) because it is a mutation the operator asked for. This is a display: the number beside a row must describe the same base the row's own base line shows, and substituting a different one answers a question the row is not asking. |
| D30 | Pull **defaults to merge**; rebase is offered beside it | Merge preserves the open PR's review anchors and needs no force-push. Rebase rewrites history and force-pushes, which is the right trade only when the operator chooses it. |
| D31 | A **dirty worktree prompts** to commit and push first | Refusing outright leaves the operator with a permanently dead button in any worktree an agent is working in; auto-stashing can conflict on the pop. Committing is the one resolution that is explicit, reversible through git, and leaves the child session's work safe. |
| D32 | A pull **pushes**; a failed push is reported, not rolled back | Without the push the GitHub PR still reports itself behind and the row contradicts the reviewer's view. The local merge or rebase having landed is a fact worth reporting truthfully. |
| D33 | Conflicts **abort**; the node is not stamped `has-conflicts` | Leaving `MERGE_HEAD` and markers in a worktree an agent may be mid-turn in would corrupt that turn. And with C4 making conflicts a live fact on every poll, a persisted stamp can only go stale — and clearing it risks stomping an agent's own `source: "override"`. |
| D34 | **One in-flight guard covers every control that mutates a row's branch**, not one per control | Merge and rebase shared a guard; repoint had its own. But repoint rebases and force-pushes the very branch a pull merges into, and the state where both controls render is the *normal* post-merge state — a parent merged, so the node both needs repointing and has fallen behind. Nothing serialises them daemon-side either. Git's `index.lock` usually makes one abort rather than corrupt, which leaves a half-rebased worktree or a force-push over a merge commit. The guard is per node, so unrelated rows stay independent. |

## Acceptance criteria

1. A collapsed row shows its title, status and call-to-action, and none of its detail lines are visible.
2. Expanding a row reveals its branch, base branch, worktree, node id, parents and child recipe.
3. Only the row whose header was clicked expands.
4. A row stays expanded across a branch-resolution poll tick and across the panel closing and reopening.
5. Start session can be clicked from a collapsed row without expanding it.
6. Clicking a spawned node's indicator selects and attaches its bound child session.
7. When the plan's recorded session is not a known session, the indicator binds to the session that owns the
   branch; when neither is known it is plain text.
8. Rows render in the order the plan persists, including where that places a node above its parent.
9. A row keeps its position when its `parents` or PR status change.
10. A plan with no persisted order renders in topological order, roots before dependents.
11. Move up / move down change a row's position and persist it; they are inert at the ends.
12. A row behind its base states how many commits, naming the base that was compared.
13. A row that would conflict shows a conflict badge and lists the conflicted paths when expanded.
14. A comparison that could not be made reads "unavailable" and renders neither "in sync" nor a behind count.
15. No base status renders while the poll has not answered, or against a daemon that reports none.
16. A row behind its base offers merge and rebase, the merge control naming the commit count.
17. Neither control is offered when the row is in sync, conflicted, unavailable, or has no branch.
18. Merge and rebase each send their own strategy and the base the control named.
19. Every control that mutates a row's branch — merge, rebase and **repoint** — is disabled while any
    one of them is in flight for that row, and only for that row.
20. A pull against a dirty worktree prompts before doing anything, and can commit and push first.
21. A refused or failed pull shows the reason inline, and the reason remains visible on a collapsed row.
22. A pull whose merge or rebase landed but whose **push** failed says so — that the work is in the branch
    and not yet on the remote — carrying the daemon's reason. A pull that reached the remote says nothing
    about it.
23. The base-sync probe leaves the working tree, the index and `HEAD` exactly as it found them, including when
    it finds conflicts.
24. A base-sync failure degrades only itself: the session, worktree, remote and PR legs stay populated.

## Non-goals

- **Drag-and-drop reordering.** Move up / move down satisfies the requirement; a drag surface is a separate
  interaction design.
- **Hash-syncing drawer selection.** Opening a session from the panel selects and attaches but does not change
  the URL — identical to the existing peer switcher and to clicking a drawer row. Making selection
  URL-addressable is a drawer-wide change.
- **New agent tools.** The reorder and pull surfaces are operator controls. The orchestrator agent's `pr_*`
  tool set is untouched.
- **A true multi-parent integration base.** A diamond node still compares against its nearest ancestor, as
  today ([PR stacking § Full DAG handling](../pr-stacking.md#full-dag-handling)).
- **Resolving conflicts from the panel.** A conflicted row reports its paths and routes the operator to the
  agent, which already has `pr_resolve_conflicts` and an editing worktree.
