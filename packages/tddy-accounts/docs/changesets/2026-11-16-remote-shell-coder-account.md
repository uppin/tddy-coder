# 2026-11-16 — Connect a Coder deployment as an account

**Type:** Feature

`tddy-accounts` gains `src/coder_account.rs`: the `coder` record codec, the
`CoderDeploymentProbe` port, and `CoderAccounts::{connect, credentials_for}` behind the new
`ConnectCoderAccount` RPC. `service.rs` gains one delegating arm and stays otherwise
unchanged. The HTTP probe is the daemon's, in `tddy_daemon::coder_deployment_probe`, because
this crate carries no network stack.

Detail: [coder-accounts.md](../coder-accounts.md). Cross-package record:
[docs/dev/changesets/2026-11-16-remote-shell-coder-account.md](../../../../docs/dev/changesets/2026-11-16-remote-shell-coder-account.md).

`service.rs` is 790 production lines (was 772); its oversized-file record carries the new
measurement and stays open for the follow-up split named there.
