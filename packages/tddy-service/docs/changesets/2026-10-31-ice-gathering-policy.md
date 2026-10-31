# 2026-10-31 — `LiveKitSettings.ice_gathering` in the daemon config surface

**Type:** Feature

`daemon_config.proto` gained `LiveKitSettings.ice_gathering` — a new `IceGathering` enum
(`ICE_GATHERING_UNSPECIFIED` / `GATHER_ONCE` / `GATHER_CONTINUALLY`), so the web Settings
form can read and set the policy [the kernel config now
carries](../../tddy-daemon-kernel/docs/changesets/2026-10-31-ice-gathering-policy.md).
Unspecified reads as `GATHER_ONCE`, the daemon's default.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
