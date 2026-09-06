# 2026-08-13 — Wrapping a changeset leaves dangling `1-WIP` pointers in code comments

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`/wrap-context-docs` deletes the WIP PRD and changeset, but code and test comments that cite them by
path are not part of its sweep, so every wrap silently leaves broken pointers behind. The
pr-stack-base-session wrap had six (a proto field comment, its generated TS copy, and four acceptance
suite headers) and they were repointed by hand at
`pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13`.

One is still outstanding from an earlier wrap: `packages/tddy-service/proto/connection.proto:449` and
its generated copy in `packages/tddy-web/src/gen/connection_pb.ts` cite
`docs/ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md`, which exists nowhere — not even
under `1-WIP/archived/`. Left alone here because it belongs to a different changeset and repointing it
churns a generated file.

Fix the process, not just the instances: the wrap step should grep the tree for the paths it is about to
delete and refuse to finish while any code reference survives.
