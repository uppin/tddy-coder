# Desktop sign-in shaped by its advertised methods, with Remember — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 8/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Screen sharing sessions](../screen-sharing-sessions.md): AC-SS-15 ("asked for, never stored"), which becomes "asked for, stored only when the operator chooses Remember", and § The desktop password is asked for, and kept nowhere

## Summary

Opening a host desktop first asks the host **which sign-in methods that desktop advertises**
(`InspectHostDesktop`). It is an on-demand handshake: the daemon runs the bridge in inspect mode,
which reads the RFB security list or the RDP negotiation response and disconnects. The sign-in form
then asks for exactly what the chosen method needs:

| Advertised | Form |
|---|---|
| VNC None | nothing; the desktop opens directly |
| VNC Authentication | password |
| VeNCrypt Plain/TLSPlain, Apple Remote Desktop | username + password |
| RDP TLS | username + password |
| RDP NLA | username + domain (optional) + password |

When several are offered, the strongest is preselected, and the operator can switch. The answer
travels encrypted on the host's prompt channel, as the password does today, now carrying a
structured credential. A **Remember** box stores the credential in the operator's credential vault,
as a screen-sharing record keyed to this host's target. The next open uses it without asking. A
desktop that refuses a remembered or typed credential re-opens the form with the server's reason,
and a refused remembered credential is not used again until replaced.

## Background

`HostDesktopOverlay` creates targets with `username: ""` and asks one password through
`HostPassphraseDialog`. Nothing knows what a desktop wants, so every open asks for a password, even
for a desktop that wants none or that also needs a username. The credential vault already holds
session-scoped screen-sharing records (`SCREEN_SHARING_PROVIDER`).

## Proposed Changes

### What's Changing
- `screen_sharing.proto`: `InspectHostDesktop(target_id) → { repeated DesktopAuthMethod offered; repeated uint32 unsupported }` and `enum DesktopAuthMethod`. `ScreenSharingTarget` / `AddHostTargetRequest` gain `domain`.
- `host.proto`: `HostPromptKind.DESKTOP_CREDENTIALS`, whose prompt carries the auth method. The encrypted answer is a structured credential (username, domain, password, remember).
- Bridges: an `--inspect` mode (`tddy-vnc` reuses node 3's security-list parser; `tddy-rdp` sends an X.224 Connection Request and reads the negotiation response).
- Daemon: `StartHostStream` uses a remembered credential when one exists, otherwise prompts with the method. With `remember`, it writes the vault record. On `Unauthenticated` it marks the remembered record refused.
- Web: `HostDesktopSignInDialog` (fields by method, method switcher, Remember), reusing the key-pinning and encryption of `HostPassphraseDialog`; re-ask on `Unauthenticated`.

### What's Staying the Same
- Unremembered credentials are kept nowhere; key pinning; the prompt TTL; no browser-side protocol client.

## Success Criteria
- [ ] A VNC None desktop opens with no form
- [ ] A VncAuth desktop asks only for a password; an ARD desktop asks for username + password; an NLA desktop asks for username, domain and password
- [ ] With Remember checked, the next open asks nothing; unchecked, nothing is written to the vault or to disk
- [ ] A refused credential re-opens the form with the reason; a refused remembered credential is not reused
- [ ] Inspect against a desktop that advertises nothing tddy supports says so and offers no form

## Out of Scope
- Editing or deleting remembered host credentials from an accounts screen (the record is visible wherever vault records are listed).
