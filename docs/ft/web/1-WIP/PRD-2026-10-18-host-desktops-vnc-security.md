# VeNCrypt and Apple Remote Desktop sign-in for VNC desktops — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 3/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Screen sharing sessions](../screen-sharing-sessions.md): the VNC bridge's authentication.

## Summary

`tddy-vnc` performs RFB version and security negotiation itself. It signs in with **None**,
**VNC Authentication**, **VeNCrypt Plain / TLSPlain** (username + password, TLSPlain over TLS) and
**Apple Remote Desktop** (type 30, username + password, Diffie-Hellman). It then hands the
authenticated connection to vnc-rs from ServerInit onward. A server that advertises types tddy does
not know is still accepted when it also advertises a known one. The advertised list is parsed by a
public function that node 8's auth inspection reuses.

## Background

vnc-rs 0.5.3 (git `ab684d0`) selects only None or VncAuth. Any other type is
`"Security type apart from Vnc Auth has not been implemented"`, and an advertised number outside
its enum (Apple's 30/35, for example) fails parsing of the whole list. macOS Screen Sharing and
VeNCrypt-only servers (TigerVNC's default) cannot be opened. The username is ignored today.

## Proposed Changes

### What's Changing
- A `security` module in `tddy-vnc`: version handshake, security-type list parsing (unknown types kept as `Unknown(u8)`), preference order, and each method's exchange.
- The bridge picks the strongest advertised method it has credentials for. A method needing a username it was not given is not chosen.
- Hand-off to vnc-rs after SecurityResult; if vnc-rs cannot start from there, a patched vnc-rs pinned by git rev (decided at green, recorded).
- New crates for ARD (bignum DH, AES-128, MD5) — **asked before adding**.

### What's Staying the Same
- Encodings, frame pump, input injection.

## Success Criteria
- [ ] Each of None, VncAuth, VeNCrypt Plain, VeNCrypt TLSPlain and ARD signs in against a fake RFB server and streams a frame
- [ ] A server advertising `[30, 2]` signs in with VncAuth when no username is given, and with ARD when one is given
- [ ] A server advertising only unknown types fails with an error naming them
- [ ] A refused SecurityResult fails `connect` as an authentication error carrying the server's reason

## Out of Scope
- VeNCrypt X509 client certificates, RA2/RA2ne, Tight security, SASL.
