# 2026-04-03 — ConnectionService: workflow files, session base path, delete

- **`ListSessionWorkflowFiles`** / **`ReadSessionWorkflowFile`**: Allowlisted basenames (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`) under **`{sessions_base}/sessions/{session_id}/`** with canonical-path checks (**`session_workflow_files`**; tests **`session_workflow_files_rpc`**).
- **Sessions base**: **`sessions_base_for_user`** resolves the Tddy **data directory** (typically **`~/.tddy`**), matching **`tddy_core::output::tddy_data_dir_path`** when **`TDDY_SESSIONS_DIR`** is unset, so listing/connect/delete target the same trees as **`tddy-coder`**.
- **`DeleteSession`**: Terminates a live **`metadata.pid`** when needed (SIGTERM/SIGKILL; Linux zombie handling), then removes the directory; directories without readable **`.session.yaml`** are removed when the resolved path is valid.
- **Package**: [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md). Web: [web-terminal.md](../../web/web-terminal.md), [web changelog](../../web/changelog/).
