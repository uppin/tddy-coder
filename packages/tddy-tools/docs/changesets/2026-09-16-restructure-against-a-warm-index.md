# 2026-09-16 — `restructure` can run against a warm index

**Type:** Enhancement

When **`TDDY_INDEX_SOCKET`** is set and non-empty, `tddy-tools restructure` runs the operation on the
`tddy-index-daemon` at that Unix socket instead of spawning its own rust-analyzer. When it is unset or
empty, the existing cold path runs unchanged — so CI is unaffected by construction. Empty is treated
as unset, matching `LIVEKIT_TESTKIT_WS_URL`.

A socket that is set but **unreachable is an error**, not a silent fall back to the cold path. A
misconfigured daemon should be visible rather than merely slow.

The warm run says which daemon served it, because for every operation that needs no language server
the two paths produce byte-identical stdout — so nothing on the console would otherwise distinguish a
daemon serving this tree from one serving the wrong one. The daemon's streamed events render to the
same console the cold path produces: the answer on stdout, the server's narration on stderr with each
line stamped by the time since the line before it. An acceptance test asserts the whole stdout vector
against the literal lines the cold path's own suite pins, and a sibling test does the same for the
stamped stderr, so the two front ends cannot drift apart unnoticed.

`--indexing-budget` is gone from `restructure apply`, `check` and `anchors`: a run waits until it
succeeds or its caller stops it, so there is no budget to state.
