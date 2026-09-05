# 2026-06-14 — **Remote-codebase mode

**Type:** Feature

tool env wiring** — `backend/mod.rs`: `RemoteToolEnv` struct with `env_pairs()`; `InvokeRequest.remote: Option<RemoteToolEnv>`; `backend/claude.rs`: exports `TDDY_REMOTE_*` env vars before subprocess spawn; `workflow/mod.rs`: `extract_remote_env_from_ctx`; `workflow/task.rs`: populates `InvokeRequest.remote` from ctx keys. Feature [remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md). (tddy-core)
