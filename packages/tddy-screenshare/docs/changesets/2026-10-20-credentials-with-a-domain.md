# 2026-10-20 — Credentials with a domain

**Type:** Feature

`Credentials { username, domain, password }` and `ScreenSharingClient::connect(host, port, &Credentials)`
replace the positional `Option<&str>` pair. `BridgeConfig.domain` is optional (`#[serde(default)]`);
`BridgeConfig::credentials()` turns each empty field into `None` and `run_bridge` signs in with it.
Cross-package record: [RDP NLA sign-in](../../../../docs/dev/changesets/2026-10-20-host-desktops-rdp-nla-sign-in.md).
