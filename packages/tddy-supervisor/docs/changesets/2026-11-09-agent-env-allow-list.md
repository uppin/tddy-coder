# 2026-11-09 — Agents start from an allow-listed environment

**Type:** Feature

`#agent-tools` 28/32 ([#836](https://github.com/uppin/tddy-coder/pull/836)).

- `policy::resolve_relayed_env`: no `allowed_env_keys` for a relayed coding agent; loader keys are still refused. `allowed_env_keys` still gates `SpawnSession` and clone spawns.
- `EnvironmentBase::Exactly`: the request's env plus the account's `HOME`/`USER`/`LOGNAME`; no supervisor `PATH` or locale.
