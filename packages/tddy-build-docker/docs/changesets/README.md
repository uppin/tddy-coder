# Changesets Applied

Wrapped changeset history for tddy-build-docker.

Each changeset is **its own file**, named `YYYY-MM-DD-<slug>.md`.

There is deliberately **no index**. The directory listing is the index: the date
prefix sorts entries reverse-chronologically, so the newest work is

```bash
ls changesets/ | sort -r | head
```

## Adding an entry

Create **one new file**. Never append to an existing one, and never add an index
that every branch would have to edit — a shared append-point is exactly what this
layout replaces (it used to be a single `changesets.md` with
`- **YYYY-MM-DD** [Type] **Title** — summary` entries, merged with git's `union` driver, which still stranded
blocks mid-file when two branches landed together).

- **Filename**: `YYYY-MM-DD-<short-slug>.md`. If the name is taken, add `-2`.
- **First line**: `# YYYY-MM-DD — Title`.
- **Body**: whatever the entry needs — prose, bullets, tables, links.
- **Never edit or reorder** an existing entry to tidy it up in the same PR as
  unrelated work. Corrections are their own commit.

Because every entry is a new file, parallel branches cannot conflict here.
