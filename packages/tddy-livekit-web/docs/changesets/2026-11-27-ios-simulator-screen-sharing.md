# 2026-11-27 — Wheel and touch input; received-video stats (PR #1060)

**Type:** Feature

`useScreenSharingInput` forwards wheel events (summed per frame, `framebufferWheelDeltaFor`) and
single touches (as the left button), both non-passive so the page does not scroll or zoom.
`useReceivedVideoStats` samples `inbound-rtp` video once a second (`receivedVideoStats.ts`).
