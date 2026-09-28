# 2026-09-28 — `useModelRegistryFanOut.ts` is over the file-length budget and still growing

**Category:** Deferred — developer consented 2026-09-28 (PR #555 `/pr-wrap` step 3.5)

`packages/tddy-web/src/components/models/useModelRegistryFanOut.ts` was already 598 production
lines against the 500 budget when PR #555 (`#subagent-control` 3/5) grew it to 615 by adding the
`usageNotes` surfaces to the create/update payloads and projected rows.

**Why deferred:** the growth this PR caused is two payload fields; a hand TS split
(`code-restructuring` v1 is Rust-only) would bury a small stacked diff mid-wrap. The developer
chose defer over decompose at wrap.

**What closing it would take:** a dedicated change hand-splitting the hook along its three
concerns — per-host RPC fan-out, projected row models, command payloads — green baseline before,
mechanical moves only, same green after. Measured record:
`packages/tddy-web/docs/code-issues/oversized-file-use-model-registry-fan-out.md` (the package's
first; the package being unanalyzed is itself flagged in PR #555's changeset).
