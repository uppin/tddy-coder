# 2026-10-23 — ListExecToolsRequest.session_id, StartSessionRequest.session_role
**Type:** Feature

`ListExecToolsRequest.session_id = 3` asks for the set that session is offered; `StartSessionRequest.session_role = 43` is honoured only on the daemon's own project-home ensure — a client start that sets it is refused (#700, `#project-home` 5/7).
