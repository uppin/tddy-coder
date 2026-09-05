# 2026-04-03 — Session workflow files + connection path alignment

**Type:** Feature

**`ListSessionWorkflowFiles`** / **`ReadSessionWorkflowFile`** (**`session_workflow_files`**, **`session_workflow_files_rpc`**). **`sessions_base_for_user`** resolves the Tddy data root (**`~/.tddy`**) so **`unified_session_dir_path`** matches **`tddy-coder`** session trees. **`DeleteSession`** terminates a live recorded PID (SIGTERM/SIGKILL; Linux zombie handling), removes directories missing **`.session.yaml`** when safe, and exposes delete on active sessions from the web client. Web: **`SessionWorkflowFilesModal`**, **`SessionMoreActionsMenu`**, project/worktree matching (**`sessionProjectTable`**). Feature docs: [web-terminal.md](../../../../docs/ft/web/web-terminal.md), [daemon changelog](../../../../docs/ft/daemon/changelog/), [connection-service.md](../connection-service.md). (tddy-daemon, tddy-service, tddy-web)
