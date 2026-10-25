# 2026-10-25 — The room terminal feed opens with a snapshot
**Type:** Bug Fix

`openRoomTerminalStream` asks for a snapshot in its first message, runs every frame through
`TerminalFrameContinuity` and asks again on the same stream after a gap; room frames now carry
`snapshot` / `startOffset` / `endOffset`, so their history fill is anchored (#763,
`#term-snapshot` 4/4).
