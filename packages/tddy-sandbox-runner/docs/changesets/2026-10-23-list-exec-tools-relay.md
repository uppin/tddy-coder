# 2026-10-23 — The jail relays ListExecTools, bound to its own session
**Type:** Feature

`conversation_root::bind_to_this_session` rewrites a relayed `ListExecTools` request to name the jail's own session with no token or daemon id, as it does for `ConversationWorktree` (#700, `#project-home` 5/7).
