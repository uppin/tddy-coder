# 2026-09-06 — `config.example.yaml`'s commented `livekit:` block is stale

**Category:** Deferred from `optional-livekit` (#449)
**Source:** optional-livekit common-room switch, #449

It uses `room:` / `identity:` /
`token:`, which matches neither `tddy_daemon::config::LiveKitConfig` nor
`tddy_coder::config::LiveKitConfig` as they stand. Pre-existing and unrelated to the switch — the
file is a tddy-coder configuration, so `livekit.enabled` correctly does not appear in it.
