# 2026-06-26 — Screen Sharing tab with VNC and RDP protocol selector

- Session inspector gains a **Screen Sharing** tab (alongside Details, Tools, and VNC) accessible from any session
- Add form has a **protocol selector** (VNC / RDP) that auto-fills the default port (5900 / 3389); selecting RDP reveals a **username field**
- First vault operation (add target with password) prompts for the vault passphrase; subsequent adds in the same session skip the dialog (`vaultUnlocked` session guard)
- **Start** calls `ScreenSharingService.StartStream`; daemon dispatches to the VNC or RDP bridge binary and opens the full-screen LiveKit overlay
- Inline error messages appear below the form on `AddTarget`, `UnlockVault`, or `StartStream` failures (no silent swallowing)
- Username stored on the target and threaded through to the RDP `IronRDP` credential handshake (was hardcoded `"user"`)
