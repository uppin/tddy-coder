# 2026-11-02 — The daemon installs the host's coding agents

**Type:** Feature

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)). `runtime/mount_host.rs` builds `tddy_agent_launch::host_coding_agents` from `<tddy_data_dir>/tools` and installs it with `with_coding_agents`. A tool file that does not load fails startup, naming the file. See the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
