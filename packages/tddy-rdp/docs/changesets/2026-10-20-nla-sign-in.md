# 2026-10-20 — NLA sign-in

**Type:** Feature

CredSSP (NTLM, no KDC) is enabled and `credentials.domain` is passed to IronRDP; the security protocol
follows the server's negotiation, so NLA and TLS-only desktops both sign in. A KDC request is refused
with an error, and a CredSSP failure from `connect` carries `NLA authentication failed`. Covered by
`tests/rdp_nla_integration.rs`. Cross-package record:
[RDP NLA sign-in](../../../../docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md).
