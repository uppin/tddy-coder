# 2026-10-23 — Project-home git tools wired into ExecuteTool
**Type:** Feature

`ExecToolRpcHandler::run_locally` routes the seven project-home git tool names to the project-home handler beside node 5's four; `DaemonProjectHome::push_identity` reads the target's project and returns `SessionAccountAccess::session_identity_in(..).git_environment` — the handler now holds the host's credential vaults and `GithubCli` for it (#702, `#project-home` 7/7).
