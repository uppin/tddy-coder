# 2026-06-26 — **VNC sessions

**Type:** Feature

VncVault + VncServiceImpl control plane** — new `vnc_vault.rs`: `VncVault` (Argon2id KDF + ChaCha20-Poly1305 AEAD; `.vnc.yaml` mode 0600; create/unlock/add_target/list_targets/remove_target/decrypt_password); new `vnc_service.rs`: `VncServiceImpl` (6 RPCs — ListVncTargets/AddVncTarget/RemoveVncTarget/UnlockVncVault/StartVncStream/StopVncStream; per-session vault-key cache; bridge-spawn FIXME); `config.rs` gains `VncConfig` with `binary_path` resolution; 7 vault acceptance tests + 5 service acceptance tests. Feature [vnc-sessions.md](../../../../docs/ft/web/vnc-sessions.md). (tddy-daemon)
