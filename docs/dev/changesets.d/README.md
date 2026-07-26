# Long-form cross-package changeset shards

One file per wrapped changeset, for cross-package work whose narrative does not fit a single line in
[`docs/dev/changesets.md`](../changesets.md).

- **Filename:** `YYYY-MM-DD-short-slug.md` (the slug is the changeset's own name).
- **The index stays authoritative.** `docs/dev/changesets.md` keeps exactly **one new bullet** per
  wrapped changeset, which links here plus to the feature docs and per-package indexes. Do not move
  the index line into this directory.
- **Why a separate file:** `docs/dev/changesets.md` is union-merged (see
  [changelog merge hygiene](../guides/changelog-merge-hygiene.md)), so two branches editing the same
  bullet still conflict. New files almost never do.
- **Scope:** the shard is the audit narrative — what was broken, what was decided, what was deliberately
  left. Durable behaviour belongs in the feature docs under `docs/ft/` and `packages/*/docs/`; a shard is
  never the only place a shipped behaviour is written down.
- Shards are append-only history: do not rewrite an existing one for later follow-up work.
