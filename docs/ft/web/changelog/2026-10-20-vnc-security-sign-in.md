# 2026-10-20 — VNC desktops that use VeNCrypt or Apple Remote Desktop can be opened

macOS Screen Sharing (Apple Remote Desktop authentication) and VeNCrypt-only servers can now be
opened by the VNC bridge. It signs in with None, VNC Authentication, VeNCrypt Plain, VeNCrypt TLSPlain
or Apple Remote Desktop, choosing the strongest method the server offers that the supplied
credentials allow. A server that also advertises security types the bridge does not know is no longer
turned away for them.

Limits: TLSPlain accepts the server certificate as presented and does not work with servers that offer
only anonymous Diffie-Hellman TLS; VeNCrypt Plain sends the password unencrypted when it is the only
subtype offered, and logs a warning; a silent server ends the sign-in after 30 seconds. The VeNCrypt
accept byte after the subtype choice is not yet confirmed against a real TigerVNC server. This
change has not yet run on CI.

Part of the `#host-desktops` stack ([#655](https://github.com/uppin/tddy-coder/pull/655)). See
[Screen sharing sessions](../screen-sharing-sessions.md#vnc-sign-in).
