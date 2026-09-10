# Development TODO

Known defects, deferred work and flagged debt — **one file per item**, the same convention as
[`docs/dev/changesets/`](../changesets/) and the product changelogs. See
[changelog-merge-hygiene.md](../guides/changelog-merge-hygiene.md) for why the repo works this way.

This directory replaced a single 2,100-line `docs/dev/TODO.md`. That file was a shared append-point:
every branch that recorded a finding edited the same lines, so two branches landing together
conflicted, and the union merge driver's repairs stranded blocks mid-file. A directory removes the
shared append-point entirely — **a branch that records a finding creates a new file, and never edits
one another branch might also be editing.**

## Writing an entry

- **Filename**: `YYYY-MM-DD-<short-slug>.md`, the date the item was found or deferred. If the name is
  taken, append `-2`.
- **First line**: `# YYYY-MM-DD — Title`.
- **Then** `**Category:**` — `Known failing test`, `Future enhancement`, or a named group such as
  `Deferred from \`optional-livekit\` (#449)`, and `**Source:**` naming the changeset or suite that
  turned it up. `**Status:** Resolved` is for the narrow case in *An entry leaves by being deleted*
  below — an item that turned out fixed with no changeset to wrap.
- **Body**: whatever the item needs. State **why** the work was deferred, not only what remains —
  that reason is what tells the next planner whether it blocks them (see below).
- **Links are relative to this directory**: `../../ft/coder/pr-stacking.md`, `../guides/ci.md`.

## There is no index

Deliberately, for the same reason the changesets directory has none: an index is a shared
append-point, which is exactly what this layout removes. The directory listing **is** the index, and
the date prefix sorts it.

```bash
ls docs/dev/todo/ | sort -r | head -20             # most recent findings
grep -rl 'connection_service' docs/dev/todo/       # everything touching a module
grep -rl 'Known failing test' docs/dev/todo/       # one category
grep -rL 'Status:\*\* Resolved' docs/dev/todo/     # still open
```

## This directory is read, not only written

Planning **Step 2b** (`.agents/skills/planning/references/planning-phase.md`) scans it before a
changeset is written, so an item sitting in the path of new work is found at planning time rather
than during `/green` — where the only choices left are to work around it, make it worse, or stop.

The two directions are complementary: what one change defers is what the next planner's scan finds.

## An entry leaves by being deleted

Because Step 2b **reads** this directory, an entry that outlives the defect it records is worse than
no entry: the next planner finds it, believes it, and plans around a problem that is already fixed.
So an entry has the same lifecycle as a changeset — it is **deleted** when the work closes it, not
archived and not annotated.

Deletion happens in the wrap, and the changeset drives it:

1. Planning Step 2b records each relevant entry under the changeset's `## Prerequisites`, **with a
   relative link to its file** — `[2026-08-02-slug.md](../todo/2026-08-02-slug.md)`.
2. When the work closes an entry, its verdict there becomes **✅ RESOLVED HERE**, naming what closed
   it. `/update-context-docs` is where that reclassification happens during development.
3. `/wrap-context-docs` deletes exactly those files, and the changeset entry it writes into
   [`docs/dev/changesets/`](../changesets/) (or a package's) names the resolved item — that is the
   audit trail, so nothing is lost by removing the file.

Two consequences:

- **An entry the wrapping changeset does not name is never deleted.** Nobody deletes a backlog entry
  on inference from a diff; the reference in `## Prerequisites` is the record of a decision.
- **Partly fixed is not fixed.** Edit the entry down to what actually remains and leave it in place.

`**Status:** Resolved` covers only the case with no changeset to wrap — an item somebody discovers
was fixed by a change that never recorded it, where the marker preserves the finding for whoever
looks next. `grep -rL 'Status:\*\* Resolved' docs/dev/todo/` therefore still lists the open items,
because resolved ones are usually simply gone.
