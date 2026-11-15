# 2026-11-15 — Session display names on the wire

**Type:** Feature

`SessionEntry.display_name = 39`, `StartSessionRequest.display_name = 49`, `SessionService.RenameSession` (`RenameSessionRequest { session_token, session_id, display_name }` → `{ display_name }`) and `ActivityService.RenameOwnSession` (`{ session_id, hook_token, os_user, display_name }` → `{ display_name }`). `RenameSession` joins `unbundle_service_split`'s closed `RESIDUAL_METHODS` list (24 methods).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
