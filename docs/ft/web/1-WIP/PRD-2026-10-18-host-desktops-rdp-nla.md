# RDP desktops that require NLA sign in, with a domain — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 2/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Screen sharing sessions](../screen-sharing-sessions.md): § Out of scope "NLA / Kerberos authentication for RDP (password auth only for MVP)" — NLA leaves that list.

## Summary

`tddy-rdp` signs in to servers that require Network Level Authentication (CredSSP), the Windows
default, using a username, an optional domain and a password. It still signs in to TLS-only
servers. The bridge's credential input becomes one `Credentials { username, domain, password }`
value, shared by both bridges and read from the bridge's stdin config, which gains an optional
`domain`.

## Background

`rdp_client.rs` sets `enable_credssp: false`, `domain: None` and a no-op `NetworkClient`. A server
that requires NLA refuses the X.224 negotiation, so most Windows machines cannot be opened.
IronRDP implements CredSSP/NTLM; tddy-rdp switched it off.

## Proposed Changes

### What's Changing
- `tddy_screenshare::Credentials { username: Option<String>, domain: Option<String>, password: Option<String> }`; `ScreenSharingClient::connect(host, port, &Credentials)`.
- `BridgeConfig.domain` (`#[serde(default)]`), so a config without it still parses.
- `tddy-rdp`: CredSSP enabled, with a real `NetworkClient` (NTLM only, no KDC). Domain passed through. The security protocol is chosen from the server's negotiation response.
- `tddy-vnc` adapts mechanically to the new signature, with no behaviour change.

### What's Staying the Same
- TLS-only RDP servers; certificate verification stays off (documented).
- The daemon never fills `domain` yet. That is node 8's form.

## Success Criteria
- [ ] Against an NLA-requiring test server, the bridge signs in with username, domain and password and streams frames
- [ ] Against the same server, a wrong password fails `connect` with an authentication error, not a hang
- [ ] TLS-only servers still sign in
- [ ] A bridge config without `domain` still parses

## Out of Scope
- Kerberos (KDC), smart cards, RD Gateway; the daemon/UI side of domain entry (node 8).
