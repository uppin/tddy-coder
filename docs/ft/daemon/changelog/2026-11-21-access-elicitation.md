# 2026-11-21 — An orchestrator asks the operator for read or write access to named paths

`#orchestra 6/9` ([#984](https://github.com/uppin/tddy-coder/pull/984)).

- **`RequestPathAccess`** names absolute paths, `read` or `write`, and why. Paths the orchestrator can
  already use at that mode are answered `in_reach` at once; the rest wait for the operator, up to ten
  minutes.
- **The operator is notified** (drawer, desktop, Telegram) and answers in the orchestrator's session
  pane with **Allow** or **Deny**. Allow grants the paths for the rest of the session.
- **Writes inside a write grant.** An orchestrator's `Write`, `StrReplace` and `Delete` act inside its
  write grants and nowhere else.

See [Project concept § Access requests](../project-concept.md#access-requests).
