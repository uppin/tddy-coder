# 2026-10-21 — `NoSessionDeltaStores`

`#satellite` 13/13 (#635). The `SessionDeltaStores` null object — every lookup `NoRoomHere` — is defined
beside the port in `service.rs` and re-exported at the crate root; the session host starts with it.
