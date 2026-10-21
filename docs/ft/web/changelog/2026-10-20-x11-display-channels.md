# 2026-10-20 — A host offers each of its X11 displays as a desktop

The Hosts screen lists one desktop channel per X11 display the host's daemon user can open, named
"Display :N", beside the VNC `:5900` and RDP `:3389` readings, and offers a Connect on each
connectable channel. Connecting to a display has the daemon start `x11vnc` for it on loopback, and
closing the desktop stops it. A host without `x11vnc` still lists its displays, with no Connect.
