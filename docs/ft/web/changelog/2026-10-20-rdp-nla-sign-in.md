# 2026-10-20 — RDP desktops that require NLA can be opened

Windows desktops with Network Level Authentication on, which is the default, can now be opened by
the RDP bridge. It signs in with a username, an optional Windows domain and a password, and still
signs in to servers that offer only TLS: the bridge follows whichever security protocol the server
negotiates.

A wrong password on an NLA desktop fails the connection with an authentication error instead of
hanging. NLA authenticates with NTLM, so a server that insists on Kerberos is refused with an error.
The bridge's stdin configuration gained an optional `domain`; configurations without it behave as
before. Asking for the domain in the UI is a later change in the same stack
([#660](https://github.com/uppin/tddy-coder/pull/660)).

See [Screen sharing sessions](../screen-sharing-sessions.md#rdp-sign-in).
