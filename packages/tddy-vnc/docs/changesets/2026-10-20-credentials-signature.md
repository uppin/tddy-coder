# 2026-10-20 — Adopts the Credentials signature

**Type:** Feature

`connect` takes `&tddy_screenshare::Credentials` and reads `credentials.password`; the username and
domain do not apply to VNC password authentication. No behaviour change. Cross-package record:
[RDP NLA sign-in](../../../../docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md).
