# WIP: Tunnel management RPC (operator daemon)

Cross-package: `tddy-service` (`tunnel_management.proto`), `tddy-daemon` (`TunnelSupervisor`, Connect RPC, OAuth loopback integration), `tddy-web` (`TunnelManagementPanel`, generated `tunnel_management_pb`), `tddy-livekit` (`rpc_scenarios` + `implementation_contract`), `tddy-rust-typescript-tests` gen.

## Validation Results (PR wrap)

### /validate-changes (equivalent)

- **Risks noted:** `std::sync::Mutex` inside async RPC handlers (short critical sections). `StartTunnel` updates supervisor state without binding operator `TcpListener` (semantic gap vs full lifecycle—document or extend). `OpenBrowserForTunnel` is acknowledge-only (browser opened by client).
- **Mitigation in branch:** Port ≥ 1024 enforced consistently; logs avoid full authorize URLs at info (lengths).

### /validate-tests (equivalent)

- Daemon acceptance: `tunnel_management_acceptance` uses real Connect router + supervisor (no placeholder).
- LiveKit: `integration_loopback_tunnel_streambytes_roundtrip_after_tunnel_manager_extract` gated on contract flag.
- Web: Cypress component covers list + open-browser RPC shape.

### /validate-prod-ready (equivalent)

- No test-only branches in production tunnel paths. No new `TODO`/`FIXME` in tunnel modules.
- Follow-ups (product): reconcile daemon auto-open browser with desktop UI-gesture policy; multi-row tunnel UI when required.

### /analyze-clean-code (equivalent)

- Replaced `Result<(), ()>` / `Result<i32, ()>` on supervisor with `()` and `i32` return types (clippy `result_unit_err`).
- `livekit_peer_discovery`: `is_none_or` per clippy; `too_many_arguments` allow on registry loop with rationale.
- `tddy-livekit-screen-capture`: cfg-gated macOS imports; dropped unused non-macOS stub (`screen_capture_granted`).

### Final re-validation

- `./dev cargo clippy --workspace -- -D warnings`: pass.
- `./dev bash -c './verify'`: pass (`cargo test -- --test-threads=1`, output in `.verify-result.txt`).

## Before merge (not wrapped here)

- Add one-line entry to `docs/dev/changesets.md` and package `changesets.md` files per [changelog merge hygiene](../guides/changelog-merge-hygiene.md).
- Cross-link from `docs/ft/web/codex-oauth-web-relay.md` and `docs/ft/desktop/tddy-desktop-electrobun.md` if behavior/API surface changes are user-visible.
- Optional: `packages/tddy-daemon/docs/` technical note for tunnel management RPC.
