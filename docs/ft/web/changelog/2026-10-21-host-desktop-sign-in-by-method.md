# 2026-10-21 — A host desktop's sign-in asks for what the desktop advertises, with Remember

Opening a host's desktop first asks the desktop which sign-in methods it offers, without signing in.
A desktop that needs no sign-in opens with no form, as does an X11 display. VNC Authentication asks
for a password. VeNCrypt, Apple Remote Desktop and RDP over TLS ask for a username and password. RDP
with NLA asks for a username, an optional domain and a password. With several methods offered, the
strongest is preselected and the operator can switch. A desktop that refuses the sign-in opens the
form again with its own reason. A desktop offering nothing tddy can sign in with says so, naming the
VNC security types it advertised. A **Remember** box keeps the credential in the operator's own
credential vault once the desktop has accepted it, so the next open asks nothing. A remembered
credential the desktop later refuses is not used again until it is replaced. Unticked, the
credential is kept nowhere. See [Screen sharing sessions](../screen-sharing-sessions.md) (AC-SS-15,
AC-SS-19).
