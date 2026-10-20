# 2026-10-20 — `RdpClient::connect` is 112 lines

**Category:** Deferred from `host-desktops-rdp-nla` (#654)
**Source:** `/analyze-clean-code` on #654 (score C), deferral approved by the developer.

`packages/tddy-rdp/src/rdp_client.rs` `RdpClient::connect` is 112 lines (107 before #654, which added the
CredSSP flag, the domain and the error-stage mapping). Extract the connector `Config` builder
(`connector_config(&Credentials) -> Config`) and the TLS-upgrade/finalize steps.

**Why deferred:** `#host-desktops` node 8 (`sign-in-form`) also edits `packages/tddy-rdp/src`, so restructuring the
file inside the stack would conflict with it. Do it as a follow-up once the stack has landed. It is a
mechanical extract; no behaviour change.
