# 2026-10-24 — The daemon launches coding-agent subagents
**Type:** Feature

`DaemonCodingAgentLauncher` (`coding_agent_launcher`, `coding_agent_as_user`, `coding_agent_relay`) starts a coding agent as the session's OS user — its own user, `setpriv` when privileged, `tddy-supervisor`'s `SpawnRelayed` when supervised — on the provider account's route, confined in a managed session; installed with `DaemonSessionHost::with_coding_agent_launcher`. See [coding-agent-launcher.md](../coding-agent-launcher.md) (#721, `#coagents` 2/8). Code issue `oversized-file-connection-service`: +7 (the launcher field and a re-export; stack-shared, row added).
