# 2026-11-15 — A project rename reaches a host that was offline only when the rename is repeated

**Category:** Deferred scope
**Source:** [2026-11-15-project-update-and-soft-delete.md](../changesets/2026-11-15-project-update-and-soft-delete.md)

## What is wrong

`UpdateProject` renames the project on this host and on every peer that answers. A host that does
not answer is listed in the response's `unconfirmed_hosts`, and it keeps the old name. Nothing
reconciles it later. The Projects screen groups rows by `project_id` and shows the first row's name,
so which name a reader sees depends on row order.

Soft delete and restore do not have this problem: `ListProjects` reconciles every host's row to the
newest delete or restore event and heals this host's row (`lifecycle_listing.rs`).

## Why it was not fixed with soft delete

The brief required that "the tombstone must win over sync" and asked nothing about names. Name
reconciliation needs its own event time on the row, so that the newest name wins in the listing
merge. That widens the data model and adds a second healing write to `ListProjects`.

## What would close it

Add `name_updated_at` to the row, stamped by `UpdateProject` and carried on `ProjectEntry`. In
`lifecycle_listing::reconciled_listing`, list every row with the newest name and adopt it into this
host's row, as the lifecycle already does.
