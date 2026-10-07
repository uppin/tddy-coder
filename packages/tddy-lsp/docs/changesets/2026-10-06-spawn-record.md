# 2026-10-06 — A registry reports the language server it starts

**Type:** Feature

`SpawnObserver`, `ProcessStart`, `ProcessToken` and `ProcessOutcome` are the seam a host implements to
learn which processes a crate starts and how each ended. `LspRegistry::with_spawn_observer` carries
one, and the registry spawns an `ObservedServerBody` that reports the server's start (program, args,
cwd, pid) and each of its four end sites — the missing output channel, a failed initialize, a server
that exits on its own, and the graceful-then-kill shutdown. `LspServerBody`'s public fields are
untouched, so a caller that starts a server with no host watching is unchanged.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-spawn-record.md).
