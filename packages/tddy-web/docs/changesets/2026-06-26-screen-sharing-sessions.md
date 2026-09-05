# 2026-06-26 — **Screen Sharing sessions

**Type:** Feature

inspector tab, vault passphrase dialog, protocol selector (VNC/RDP)** — `InspectorTabs.tsx`: "Screen Sharing" tab with `data-testid="sessions-inspector-tab-screen-sharing"`; `SessionInspectorDrawer.tsx`: wires all 6 `ScreenSharingService` RPCs; new `SessionScreenSharingTab.tsx` (target list, Add form with VNC/RDP protocol selector, auto-port default, RDP-conditional username field, passphrase dialog flow, `vaultUnlocked` guard prevents re-prompting, inline `<p role="alert">` error display); new `ScreenSharingOverlay.tsx` (LiveKit video track, full-screen fixed); new `ScreenSharingPassphraseDialog.tsx`; new `screenSharingTabState.ts` (reducer: add/remove/open_overlay/close_overlay/set_stream_status); proto regen (`screen_sharing_pb.ts`, `screen_sharing_input_pb.ts`); 9 Cypress component acceptance tests + 6 state unit tests. Feature [screen-sharing-sessions.md](../screen-sharing-sessions.md). (tddy-web)
