# 2026-11-10 — new crate: tddy-users-file

**Type:** Architecture

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

The `users.yaml` model both readers share: `UsersMapping::from_yaml` (shape, no empty login, no
`root`, one OS user per login, at most 15 distinct OS users, each OS user one safe path and
descriptor-name segment), `UsersMapping::distinct_rows` and `is_one_safe_segment`, with
`UsersFileRefusal` carrying the supervisor's wording. serde/serde_yaml only; no file reading, no
ownership check — each reader (`tddy-supervisor`, `tddy-daemon-kernel`) keeps its own trust check.
