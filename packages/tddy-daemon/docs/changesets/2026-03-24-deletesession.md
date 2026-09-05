# 2026-03-24 — DeleteSession

**Type:** Feature

`DeleteSession` removes an inactive session directory under the caller’s sessions tree; `session_deletion` validates ids and path containment; generic internal error on `remove_dir_all` failure with full detail in logs. (tddy-daemon)
