# 2026-10-31 — The Settings form offers the ICE gathering policy

**Type:** Feature

The daemon Settings screen's LiveKit block gained an `ICE gathering` select — `Gather once
(recommended)` / `Gather continually` — fed by the new `LiveKitSettings.ice_gathering`
proto field. `toFormState` shows the configured policy and reads an unspecified value
(a configuration predating the key) as `gather_once`, the daemon's default; the update
always sends the operator's choice explicitly, because an unspecified proto3 enum would be
indistinguishable from "did not reach the wire". The control's hint names the trade-off:
continual gathering keeps a session alive across a network switch without a reconnect, and
leaks file descriptors on hosts whose networks change. Covered by four unit tests on the
form mapping.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
