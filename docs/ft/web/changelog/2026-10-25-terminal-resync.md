# 2026-10-25 — A terminal that drifts repairs itself

The browser terminal checks that every frame continues the screen it is showing. A frame that does
not — bytes lost or repeated on a slow link — is no longer painted over the screen; the terminal
keeps the last consistent picture, asks the host for a fresh snapshot on the same connection, and
repaints when it arrives. The host also sends one when the terminal falls behind its output. See
[Web terminal](../web-terminal.md).
