# 2026-11-21 — A target tab that cannot start says why

A target [tab](../session-liveness.md#a-target-sessions-tabs) that cannot start reports one error whichever kind it
is: an unreachable target is `UNAVAILABLE` (agent tabs included), a local terminal that cannot spawn is `INTERNAL`,
and an agent the target's login shell does not find is `FAILED_PRECONDITION` naming the command — checked before
any terminal opens, for new and restored agent tabs alike, instead of a tab that exits `127`.
