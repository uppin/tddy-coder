# 2026-10-29 — A grill-me session reads its children; the project-home main notifies its operator

- **A conversation spawner reads the sessions it started.** A `grill-me` session is offered
  `ReadSessionActivity`, so it sees the `request_id` of its child's permission prompt or question and
  answers it at once, instead of the child waiting out `answer_wait_secs`. Reading a session it did
  not start is refused as permission denied, saying why. Same host only, for now.
- **The project-home main tells its operator.** It has no shell for `tddy-tools notify`, so it is
  given the `NotifyOperator` tool: the same title, markdown body, level and sound, published as the
  same agent notification on Telegram and the desktop. Sessions without the role are not offered it.
- **Answering a permission no longer clears a newer question.** The pending question is cleared only
  if it is still the one answered, as an answered question already was.

([#788](https://github.com/uppin/tddy-coder/pull/788)) See
[project-concept.md § Conversation spawners](../project-concept.md#conversation-spawners) and
[session-notifications.md § A session with no shell](../session-notifications.md#a-session-with-no-shell-notifyoperator).
