# 2026-06-14 — Remote-codebase mode

**Type:** Feature

`remote.rs`: `RemoteContextDir` RAII, `build_remote_allowlist`, `REMOTE_APPENDIX`; `config.rs`: `RemoteConfig` (daemon_url/session_id/session_token/daemon_instance_id), `to_remote_tool_env()`; `run.rs`: `--remote-daemon-url`/`--remote-session-token`/`--remote-daemon-id` flags, `run_remote` dispatch + implementation (shells out to `tddy-tools remote list-tools`, builds allowlist, runs free-prompting workflow). Feature [remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md). (tddy-coder)
