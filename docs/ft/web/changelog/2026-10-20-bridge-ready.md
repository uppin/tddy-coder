# 2026-10-20 — A wrong desktop password says so, and clicks land where aimed

Opening a remote desktop no longer waits on "Connecting…" forever when the desktop turns the sign-in
down. The viewer says "The desktop refused the sign-in" with the desktop's own reason, on the Hosts
screen and on a session's target row. A desktop that cannot be reached, or whose bridge gives up or
never answers within 30 seconds, fails the start with its reason too, and nothing is left running.
Pointer input is scaled into the desktop's real size rather than an assumed 1920×1080, so clicks land
right on any resolution. VNC Authentication refusals are reported like the other VNC sign-in methods.

Part of the `#host-desktops` stack ([#658](https://github.com/uppin/tddy-coder/pull/658)). See
[Screen sharing sessions](../screen-sharing-sessions.md#ac-ss-5-start-stream).
