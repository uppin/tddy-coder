# 2026-10-07 — Screen-sharing targets live in the credential store; no passphrase prompt

Adding a screen-sharing desktop no longer asks for a vault passphrase: the password is sealed in the
signed-in user's credential store, together with the desktop's label, host, port, protocol and
username. Saved desktops belong to the user, so a desktop added in one session is listed in the
next, and they propagate between the user's daemons like any other credential. A store sealed under
a different login is shown as locked rather than as an empty list. Desktops saved before this are
not carried over and are added again once.

See [screen-sharing-sessions.md](../screen-sharing-sessions.md).
