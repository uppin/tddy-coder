# 2026-10-07 — The daemon holds no screen-sharing key cache

**Type:** Architecture

`runtime::build` no longer constructs a `ScreenSharingKeyCache`; it registers the screen-sharing
service with `with_credential_vaults(auth_result.credential_vaults.clone())`, so targets are records
in the caller's credential vault. `runtime.rs` shrinks from 1,781 to 1,777 production lines
(`oversized-file-runtime`). See [daemon-endpoint.md](../daemon-endpoint.md#screen-sharing).
