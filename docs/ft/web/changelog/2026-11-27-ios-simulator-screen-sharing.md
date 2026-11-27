# 2026-11-27 — Stream and control an iOS Simulator from the Hosts screen

On a Mac running a tddy daemon or Tddy Desktop, every booted iOS Simulator appears on that host's
row as a desktop, named by device and runtime ("iPhone 16 Pro · iOS 18.2"). **Connect** streams the
device screen with no window chrome and no sign-in; clicks and drags become touches, scrolls become
swipes, and typing enters text. An agent on the same Mac deploys with `xcrun simctl install|launch
<udid>` and the result shows in the shared screen. Requires Xcode, Screen Recording and
Accessibility permission, and the device's window visible; a missing permission or a device that is
not booted fails the start with a message naming it. Tddy Desktop's own host offers Connect when
the common room reaches it.

Every remote desktop (VNC, RDP, iOS Simulator) also gains **scroll-wheel and touch-screen
forwarding**, a picture fitted to the desktop's aspect (a phone fills the height), a bitrate / size /
frame-rate readout, and correct colours (they were washed towards pink).

See [Screen sharing sessions § iOS Simulator](../screen-sharing-sessions.md#ios-simulator).
