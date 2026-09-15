# 2026-09-14 — Remote git pack spawn over SSH

**Type:** Feature

`RemoteGitService.Serve` resolves [`PackExecution`] from session metadata and spawns pack verbs
locally (`setpriv` + scrubbed env) or on an SSH Host alias (piped `ssh`, remote `git upload-pack` /
`git receive-pack`). `GitChildRelay::spawn_pack_verb` owns the OpenSSH argv shape.
