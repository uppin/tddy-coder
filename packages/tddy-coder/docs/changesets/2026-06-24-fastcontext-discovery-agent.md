# 2026-06-24 — **FastContext discovery agent

**Type:** Feature

CLI wiring** — `create_backend("fastcontext")` arm via `SharedBackend::from_arc(Arc::new(FastContextBackend::new(...)))`; `fastcontext_url`/`fastcontext_max_turns` config fields; `--agent fastcontext`/`--fastcontext-url`/`--fastcontext-max-turns` CLI args; `dev.daemon.yaml` `allowed_agents` entry. Feature [discovery-agent.md](../../../../docs/ft/coder/discovery-agent.md). (tddy-coder, tddy-discovery)
