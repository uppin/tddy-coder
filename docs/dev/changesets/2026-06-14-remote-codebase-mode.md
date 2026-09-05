# 2026-06-14 — Remote-codebase mode

**Type:** Feature

workspace sessions (`ExecuteTool`, `ListExecTools` RPCs, `session_type:"workspace"`); relay daemon (`--relay`, `forward_to_peer`, per-peer `RpcClient` cache, `IdleTimeoutTracker` auto-shutdown); `tddy-tools remote` subcommands (`list-tools`, `start-session`, `connect-session`, `sync-context`); `tddy-coder --remote` (`run_remote`, `RemoteConfig`, `RemoteContextDir`, dynamic allowlist); `RemoteToolEnv` + ctx-key wiring in `tddy-core`; `remote_codebase_allowlist` in tddy-workflow-recipes. Feature [remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md); product [daemon/changelog/](../../ft/daemon/changelog/). (tddy-service, tddy-daemon, tddy-tools, tddy-coder, tddy-core, tddy-workflow-recipes)
