# 2026-06-26 — VNC sessions: inspector tab, encrypted vault, full-screen overlay

- Session inspector drawer gains a **VNC** tab alongside Details and Tools; accessible regardless of session connection state
- `SessionVncTab` lists configured VNC targets (label, host:port, per-target status) and provides an Add form (label, host, port, optional password)
- First vault operation (add with password, start stream) triggers `VncPassphraseDialog`; passphrase creates/unlocks the vault (Argon2id + ChaCha20-Poly1305 AEAD, `.vnc.yaml` mode 0600); derived key cached in daemon memory for the session
- Per-target **Start** calls `VncService.StartVncStream`; daemon spawns a `tddy-vnc` bridge binary that publishes a LiveKit video track; per-target **Stop** calls `StopVncStream` and tears down the process
- **VNC overlay**: full-screen (`fixed inset-0 z-50`) darkened overlay renders the remote desktop video; dismiss via Escape, backdrop click, or close button
- Per-target **Remove** calls `VncService.RemoveVncTarget` and deletes the encrypted credential
- New `tddy-vnc` package scaffolded with `common.rs` (`char_to_keysym`, `rgba_to_abgr`); bridge pump loop and VncClient/VncStreamer are follow-up stubs (FIXME)
- Feature: [vnc-sessions.md](../vnc-sessions.md)
