# 2026-10-22 — Harness profiles: agent definitions that name a provider account

**Type:** Feature

`#provider-route` 8/9 ([#669](https://github.com/uppin/tddy-coder/pull/669)), across `tddy-discovery`
(`harness_profile`: `HarnessProfile`, `load_harness_profiles`, `parse_harness_profile`,
`harness_profile_rows`, `names_a_harness`; `load_agent_defs` classifies each file and skips profiles),
`tddy-daemon-rpc` (`ListAgents` appends profile rows from `<tddy_data_dir>/agents`) and
`tddy-agent-launch` (`harness_profile_start::profile_start`; `harness_profile_request` wraps
`start_session_core` around the renamed `start_session_on_request`, feeding
`RouteSources.agent_definition` and writing the resolved account id; `provider_account_lookup` matches
an account by id then unique label). Validation: one gap round — the start wiring was missing and was
added at /green re-entry (25e0710a2); a `match bool` → if/else fix; no TODO/FIXME. The
`drain-tddy-discovery` backlog entry stays open (the profile module is a sibling of `agent_def.rs` so the
drain carries it). Feature: [Harness profiles](../../ft/daemon/harness-profiles.md).
