# Changelog and changeset hygiene

Changelogs and changeset histories are **directories of one file per entry**, not single
append-only documents. A branch that records a change **creates a new file**; it never edits a
file another branch might also be editing. That is the whole mechanism, and it makes a merge
conflict in this part of the repo structurally impossible.

| What | Where | One entry is |
|---|---|---|
| Cross-package changesets | `docs/dev/changesets/` | `YYYY-MM-DD-<slug>.md` |
| Package changesets | `packages/<pkg>/docs/changesets/` | `YYYY-MM-DD-<slug>.md` |
| Product changelogs | `docs/ft/<area>/changelog/` | `YYYY-MM-DD-<slug>.md` |

Each directory carries a `README.md` describing the convention. The README is the only shared
file, and nothing routine touches it.

## Writing an entry

- **Filename**: `YYYY-MM-DD-<short-slug>.md`, the date the work landed. If the name is already
  taken, append `-2`.
- **First line**: `# YYYY-MM-DD — Title`. For a changeset, follow it with `**Type:** Feature`
  (or `Fix`, `Architecture`, …) on its own line.
- **Body**: whatever the entry needs. There is no longer a one-bullet-one-line rule — that
  existed only so the union merge driver could combine independent additions, and it is gone.
  Write prose, bullets, tables, or links as the change deserves.
- **Links are relative to the entry's own directory**, one level deeper than the old files:
  `../../ft/coder/pr-stacking.md`, not `../ft/coder/pr-stacking.md`.

## There is no index

Deliberately. An index is a shared append-point, and a shared append-point is exactly what this
layout removes — reintroducing one would bring back the conflicts (and the stranded mid-file
blocks that the union driver produced when two branches landed together).

The directory listing **is** the index. The date prefix sorts it:

```bash
ls docs/dev/changesets/ | sort -r | head -20        # most recent cross-package work
ls docs/ft/coder/changelog/ | sort -r | head        # most recent coder release notes
grep -rl 'pr-stack' docs/dev/changesets/            # everything touching a topic
```

## Do not rewrite history

Never reformat, reorder, retitle, or "tidy up" existing entries in the same PR as unrelated
feature work. A correction to a shipped entry is its own commit, ideally its own PR. Entries are
an audit trail: they describe what was true when they were written, and a stale link inside an
old entry is not a bug worth a drive-by fix.

## Wrap workflow (`/wrap-context-docs`)

Wrapping **adds one new file per target directory** — nothing else in those directories is
touched. Transfer the detailed end state into the permanent feature and package docs first; the
changeset entry is a pointer and a record, not the full story.
