# 2026-10-22 — Harness profiles in ListAgents

**Type:** Feature

`#provider-route` 8/9 ([#669](https://github.com/uppin/tddy-coder/pull/669)). `catalog/ports.rs`
`list_agents` appends `tddy_discovery::harness_profile::harness_profile_rows` for the profiles in
`<tddy_data_dir>/agents`, after configured agents and registry assistants. See
[Harness profiles](../../../docs/ft/daemon/harness-profiles.md).
