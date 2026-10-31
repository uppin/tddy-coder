# 2026-10-31 — The settings mapping reads and adopts the ICE gathering policy

**Type:** Feature

`redacted_settings` reports the configured [`ice_gathering`
policy](../../tddy-daemon-kernel/docs/changesets/2026-10-31-ice-gathering-policy.md) as the
proto enum so the Settings form shows what the daemon runs; `merged_livekit` maps an
update's value back, with unspecified reading as `GatherOnce` — the same default an absent
YAML key serialises to. A changed policy joins `enabled`, `url` and `common_room` in the
common-room connection identity, so saving a new policy rebuilds the live connection
instead of leaving it joined under the old one. Four tests cover the projection, the
adoption, the unspecified default and the reconnect.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
