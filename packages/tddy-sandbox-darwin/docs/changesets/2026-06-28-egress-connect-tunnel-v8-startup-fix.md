# 2026-06-28 — Egress CONNECT tunnel + V8 startup fix

**Type:** Feature

egress shim upgraded from probe-only to an in-jail `HTTPS_PROXY` CONNECT proxy relaying raw TLS bytes over `SessionChannel` `Tunnel{Open,Data,Close}` frames (host opens the real socket via `sandbox_session.rs::spawn_tunnel`; TLS end-to-end). `(allow file-read*)` added to the SBPL template — the rule that lets the V8/Node `claude` binary boot (read confinement traded for write confinement; tech debt). Acceptance `sandbox_runner_tunnels_https_proxy_connect_via_session_channel`; confinement test updated to pin the read/write trade-off. Daemon `StartSession` egress path reuses the shared helpers — daemon-specific acceptance test pending. (tddy-sandbox-darwin, tddy-service, tddy-daemon, tddy-testing-commons)
