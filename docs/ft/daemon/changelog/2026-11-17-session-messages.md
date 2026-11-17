# 2026-11-17 — Sessions send each other messages, read as activity

A session can now answer its orchestrator, and an orchestrator can leave its sessions something
for later, without typing into anyone's terminal: `PostSessionMessage` records a message as
**activity** — a `message` record of the project notification log, stored there once — and the
recipient reads it when it next looks. Unlike `SendSessionMessage` (unchanged: orchestrator-only,
a pasted turn, needs `turn_input`), the new tool is offered to **every** session role, including
agents that declare no `turn_input` at all, such as Cursor — until now they could not be told
anything.

Who may address whom is the daemon's rule, read from the sessions' own records: one project, one
OS user, one host, never oneself; an orchestrator may address any session it can see, a session
its orchestrator or the project's main session, and a reply (`reply_to`) the sender of a message
it received. Each refusal names the reaches. A message's text is non-empty and at most 8 KiB.

The message reaches its recipient two ways, both references into the log — nothing of the message
is kept anywhere else:

- **The inbox watch.** `CreateSessionWatch` with the kind `message` alone and no session selected
  creates the caller's inbox watch — the messages addressed to it, whoever sent them. It is the
  one watch a session that is not an orchestrator may hold (every other watch tool call from such
  a session is still denied), and it is delivered like any watch: on the holder's next tool answer,
  pasted by the idle wake, or streamed by `tddy-tools monitor --kinds message`, which prints each
  message as a `message` line carrying the text whole (JSON escapes its newlines, so it stays one
  stdout line).
- **`ReadSessionMessages`** answers the messages addressed to the caller that it has not read yet,
  oldest first, at most 50 per call, through its **inbox position** — a `seq` into the log kept
  beside its watches in `session-watches.json`, advanced past what is answered.

A message addressed to a session is delivered to it **once** across its watches and its reads.
Messages are never attention notifications for a person: the drawer, Telegram and Tddy Desktop's
native notifications skip the source, and `ReadProjectNotifications` /
`WaitProjectNotifications` never return one. For the CLI there is `tddy-tools send --to
<session-id> [--reply-to <message-id>] [<text> | -]`, which prints the message's id as JSON and
exits non-zero with the reason when refused. A message shares the log's 1 000-record window with
attention records; an evicted, undelivered message is counted in `lost`. See
[Project concept § Messages between sessions](../project-concept.md#messages-between-sessions).
