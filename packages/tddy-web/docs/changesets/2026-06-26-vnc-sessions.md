# 2026-06-26 — **VNC sessions

**Type:** Feature

inspector VNC tab, VncOverlay, passphrase dialog** — `InspectorTabs`: "vnc" tab button; `SessionInspectorDrawer`: VncService client via `useHttpClient`, renders `SessionVncTab`; `SessionVncTab`: target list with per-row Start/Stop/Remove, Add form (label/host/port/password), passphrase-gated submit; `VncOverlay`: full-screen fixed overlay (`inset-0 z-50`), Escape/backdrop-click/close-button dismiss, `videoTrack.attach/detach` in useEffect; `VncPassphraseDialog`: passphrase input with confirm/cancel; `vncTabState.ts` reducer (add/remove/open_overlay/close_overlay/set_stream_status); `vncInput.ts` (scaleCoordinates, keyboardEventToKeysym, mouseButtonToRfbMask, wheelDeltaToRfbMask); 5 Cypress CT (SessionInspectorVncAcceptance) + 4 (VncOverlay) + 5 (SessionVncTargetRows); 5+15 bun unit tests. Feature [vnc-sessions.md](../vnc-sessions.md). (tddy-web)
