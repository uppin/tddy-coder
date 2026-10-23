# 2026-10-23 — Project-home tools wiring
**Type:** Feature

`DaemonRpcFamilies::attach_session_start` (default no-op) hands the families a launch snapshot of the host, held through `WeakRpcFamilies`; `svc_list_exec_tools_wiring::list_exec_tools_from_jail` answers a jail-relayed `ListExecTools` by the session's recorded role. See [session-service.md](../session-service.md) and [module-layout.md](../module-layout.md) (#700, `#project-home` 5/7). Code issue `complexity-daemon-rpc-handler-handle-rpc`: 185 → 188 (one arm; row added).
