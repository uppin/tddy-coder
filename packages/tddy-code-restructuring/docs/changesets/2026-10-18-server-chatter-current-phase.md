# 2026-10-18 — `ServerChatter::current()`
**Type:** Feature

`ServerChatter` tracks the phase reported on most recently and the highest percentage each in-flight
progress token reached, exposed as `current()` beside the unchanged `furthest()`. A token's figure is
dropped at its `end`. Consumer: the index daemon's `Warm` stream.

Docs: [readiness-and-gates.md](../readiness-and-gates.md).
