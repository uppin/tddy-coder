# 2026-10-20 — VNC sign-in with VeNCrypt and Apple Remote Desktop

**Type:** Feature

`#host-desktops` 3/8 ([#655](https://github.com/uppin/tddy-coder/pull/655)). The VNC bridge negotiates
RFB security itself and signs in with None, VNC Authentication, VeNCrypt Plain, VeNCrypt TLSPlain and
Apple Remote Desktop, then hands the stream to vnc-rs from ServerInit with no vnc-rs patch.

Only `tddy-vnc` changed; `Credentials` and the `connect` signature are untouched. Details, module layout
and open limits: [VNC security negotiation](../../packages/tddy-vnc/docs/changesets/2026-10-20-vnc-security-sign-in.md).
Feature doc: [Screen sharing sessions](../ft/web/screen-sharing-sessions.md#vnc-sign-in).

Open limits: VNC Authentication refusals are not `SecurityError::Refused` (needs a DES crate, not
approved); the VeNCrypt accept byte is unconfirmed against a real TigerVNC server; CI has not yet run.
No backlog entries or code-issue records were resolved.
