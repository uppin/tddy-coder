# Changesets Applied

Wrapped changeset history for tddy-sandbox-darwin.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-28** [Feature] **Egress CONNECT tunnel + V8 startup fix** — egress shim upgraded from probe-only to an in-jail `HTTPS_PROXY` CONNECT proxy relaying raw TLS bytes over `SessionChannel` `Tunnel{Open,Data,Close}` frames (host opens the real socket via `sandbox_session.rs::spawn_tunnel`; TLS end-to-end). `(allow file-read*)` added to the SBPL template — the rule that lets the V8/Node `claude` binary boot (read confinement traded for write confinement; tech debt). Acceptance `sandbox_runner_tunnels_https_proxy_connect_via_session_channel`; confinement test updated to pin the read/write trade-off. Daemon `StartSession` egress path reuses the shared helpers — daemon-specific acceptance test pending. (tddy-sandbox-darwin, tddy-service, tddy-daemon, tddy-testing-commons)
- **2026-06-27** [Feature] **Darwin Seatbelt sandbox spawn** — SBPL template `profiles/sandbox-claude.sb.tmpl`, `render_profile`, `sandbox-exec` spawn, path canonicalization, `(deny network*)` + loopback/unix-socket exceptions; troubleshooting [troubleshooting.md](./troubleshooting.md). Feature [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md). (tddy-sandbox-darwin, tddy-sandbox)
