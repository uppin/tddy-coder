# 2026-11-15 — The split start's code is cut into named steps (no behaviour change)

The split-session start path (`spawn_split_agent`, the argv builder and the session-list row
enrichment) is decomposed into named steps, closing the code issues the `#split-alive` stack grew.
Nothing a developer or operator sees changes. See
[2026-11-15-split-alive-sweep](../../../dev/changesets/2026-11-15-split-alive-sweep.md).
