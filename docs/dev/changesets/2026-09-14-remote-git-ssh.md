# 2026-09-14 — Serve remote-git packs through the session's shell

**Type:** Feature

`RemoteGitService.Serve` resolves pack spawn from session `ssh_config_host` metadata: local
`setpriv` git children when unset, OpenSSH to the session worktree on the SSH target when set.
`GitChildRelay::spawn_pack_verb` implements the RemoteShell argv shape without depending on
`tddy-tool-engine`.
