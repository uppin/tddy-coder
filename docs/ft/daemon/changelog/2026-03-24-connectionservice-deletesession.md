# 2026-03-24 — ConnectionService: DeleteSession

- **`DeleteSession`**: Removes the on-disk session directory under the authenticated user’s **`{sessions_base}/sessions/{session_id}/`** tree. Rejects invalid session ids with **`INVALID_ARGUMENT`**. Filesystem removal errors return a generic **`INTERNAL`** message to clients; full error detail is logged on the server.
- **Current behavior** (terminate live processes, metadata-less directories, **`sessions_base`** resolution): see **2026-04-03 — ConnectionService: workflow files, session base path, delete** above.
