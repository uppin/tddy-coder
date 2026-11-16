# 2026-11-15 — Four cross-host logic suites run over the in-memory hub

**Type:** Refactor

`relay_e2e_acceptance` (4), `session_attach_cross_host_acceptance` (8, including the
silenced-peer forward-deadline test), `multi_host_acceptance` (6) and
`staging_forwarding_acceptance` (2) were rewired from a real LiveKit room to
`tddy-room-testkit`'s hub. Assertions are unchanged; only the transport below the ports
changed. The suites now run in well under a second with no Docker, in the default unit run.
`tests/test_placement.rs` records the conversion.

Cross-package entry: [docs/dev/changesets/2026-11-15-in-memory-common-room.md](../../../../docs/dev/changesets/2026-11-15-in-memory-common-room.md).
