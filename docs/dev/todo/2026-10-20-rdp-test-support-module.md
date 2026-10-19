# 2026-10-20 — RDP integration suites duplicate their fake desktop

**Category:** Deferred from `host-desktops-rdp-nla` (#654)
**Source:** `/red` and `/validate-tests` on #654.

`packages/tddy-rdp/tests/rdp_nla_integration.rs` repeats the QR display, QR decode and TLS helpers of
`tests/rdp_client_integration.rs`. Extract a shared `tests/support/` module for the fake desktop. Optional
alongside it: a test that a wrong domain is refused by the TLS-only desktop, and one for a partly filled
`Credentials`.

**Why deferred:** both suites were green with their own copies; extracting after both pass keeps the
diff reviewable.
