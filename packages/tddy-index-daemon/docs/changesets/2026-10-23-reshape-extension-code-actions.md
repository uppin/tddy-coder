# 2026-10-23 — Verify judges installed languages' changed files; routing and verify split out (`#reshape` 23/24, #682)

**Type:** Feature

`Verify` builds the verifies of each touched installed language over its warm server and answers `VerifyResponse.source_findings` (field 9, `holds` false on any finding); `with_verified_languages`; `HostLanguages`. Engine moves: `index/routing.rs` (`Route`, `ServedFile`, `route_of`, `client_for_route`, `holds_a_server_for`, `warmth_of`) and `queries/verify.rs`. `index.rs` 605 → 495 and `queries.rs` 492 → 442 production lines; the `oversized-file-index` code issue is closed (final 495).

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
