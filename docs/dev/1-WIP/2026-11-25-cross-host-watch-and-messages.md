# Watches, messages and session lists across connected hosts

**Type:** Feature
**Work items:** n5 (cross-host watches and messages), n6 (every connected host's sessions in the
session list)
**Packages:** `tddy-session-activity`, `tddy-daemon-rpc`, `tddy-tool-engine`, `tddy-tools`,
`tddy-daemon-kernel`, `tddy-service`, `tddy-agent-launch`, `tddy-host-service`, `tddy-daemon`
(wiring), `tddy-session-web`

## State A

An orchestrator could already **start** a session on another connected daemon and **read** it
(`ReadSessionTranscript`, `ReadSessionActivity`, the terminal and git tools, `AnswerSessionQuestion`,
`AnswerSessionPermission`, `SendSessionMessage`): every target tool given `daemon_instance_id` is
forwarded whole to that host over the peer forward, which confirms the caller at its owner.

What it could not do:

- **Watch** such a session. Watches read only this host's project notification log, and a child
  on another host writes its events into *that* host's log. `started_by_me` never saw a remote
  child; `session_ids` naming one was refused ("no session record here"). So its questions,
  permissions, turn ends, endings and messages were never delivered to the orchestrator.
- **Message** across hosts. `PostSessionMessage` read the recipient's record on the sender's host
  and refused anything it did not hold, so a child's `tddy-tools send` to an orchestrator on
  another host was refused as not addressable, and so was the reverse.
- **List** another host's sessions in the drawer. `ListProjectSessions` already fanned out to every
  eligible host and named each row's `daemon_instance_id`, but carried no readable host label; the
  web drawer called `ListSessions` on the selected daemon only and showed other hosts' sessions only
  while they were live room participants.

## State B

### The notification relay (n5)

Each host's project notification log stays the source of truth for its own sessions. A record that
concerns a session held on another host is **pushed** there by a relay
(`tddy_session_activity::notification_relay::NotificationRelay`), and appended to that host's own
log — so every watch, inbox, wake, long poll and stream there reads it unchanged. A record concerns
another host when:

- its session's **orchestrator runs there** — the session's changeset names
  `orchestrator_daemon_instance_id`, written by the start before the agent runs;
- it is a **message addressed to a session there** — `SessionMessage::to_daemon_instance_id`;
- a **watch there follows the session by name** — registered by that host's `CreateSessionWatch`
  (and again by `RenewSessionWatch`) until the watch expires.

Only a host's own records are relayed: a relayed record never travels a second hop.

**Delivery.** The outbox is the log itself. The relay keeps a cursor per (project, destination
host), persisted in `<data dir>/notification-relay.json`, and advances it only when the host
acknowledges a batch (at most 100 records, one OS user). While a host is unreachable the relay
retries (`RelayTimings::retry`, 5 s in production); nothing is dropped while the records stay in
the 1 000-record window. A cursor the window has passed is reported in the next batch as `lost` —
an upper bound, since an evicted record's destination is no longer known — and the receiver
appends a **relay gap** record carrying the count, which every watch reports once in its `lost`,
naming nothing as an event. The receiver remembers, per origin host, the highest origin `seq` it
holds (recovered from the records on restart), so a batch resent after a lost acknowledgement
appends nothing twice. Both sides persist; a restart on either side resumes.

**Transport.** No new transport and no new RPC: a batch is an `ExecuteTool` call addressed at the
destination host under the internal name `tddy.relay.SessionNotifications` (a follow under
`tddy.relay.FollowSession`), forwarded through `PeerRouting::rpc_served_by_peer` exactly as a target
tool call is. These names are **never tools**: no listing or schema offers them; `ExecuteTool`
refuses them from any transport but the common room — an agent's call is refused on its own host,
before any forward — and from a peer whose call is not a relay call (a session id or token present,
addressed at another host, or a body that is not a relay request). `StreamExecuteTool` refuses them
outright.

**Trust.** A relayed batch names the GitHub login of the records' OS user; the receiver maps it to
its own OS user, and accepts a record only when it is relayed for a session it holds in the batch's
project (the orchestrator, the message's recipient, or the watch's owner). Peers in the common room
are trusted daemons, as for `IsSessionTerminalServed`.

**Records.** `ProjectNotificationRecord` gains `daemon_instance_id` (the host the session runs on;
`None` for this host's own), `origin_seq`, and `relay_lost` (non-zero only on a relay gap). Every
record an orchestrator is handed — watch events on tool answers, `WaitSessionWatchEvents`, the
monitor's lines, `ReadProjectNotifications` / `WaitProjectNotifications` — names its host, this
host's id for its own sessions.

**Wake.** A relayed batch cues the idle-wake waker directly with its project (it never rides the
bus), so a remote child's event wakes an idle orchestrator watching with `wake_when_idle`.

### Watches (n5)

- `started_by_me` covers children on every connected host, with no extra step: their hosts relay
  their records to the orchestrator's host.
- `session_ids` may name a session on another host. `CreateSessionWatch` finds it through the same
  peer listing `ListProjectSessions` uses, and registers a follow there; a session no connected host
  lists is refused naming it, and a follow the host refuses cancels the watch and names the host.
  `RenewSessionWatch` registers the follow again for the renewed expiry.
- The tool description no longer says "on this host".

### Messages (n5)

`PostSessionMessage` (and `tddy-tools send`) take an optional `daemon_instance_id` (`--host`). The
addressing rule is unchanged — one project, one OS user, never oneself, one of the caller's reaches
— and is applied wherever the recipient runs. The recipient's host is found, in order: the caller's
own orchestrator on the host its changeset names; the sender of the message replied to on the host
it was relayed from; this host when it holds the recipient; otherwise every connected host (or the
named one) is asked `ListSessions` with the caller's token, and only a row a host reports for itself
is believed. The first two need no network, so a message to them is **queued** in the sender's log
while the link is down and delivered once it is back. A host the room does not list, or a named host
that cannot be asked, is an **explicit refusal** naming it — a message is never queued for a host
nobody can confirm. `ReadSessionMessages` rows and the monitor's message lines gain
`from_daemon_instance_id`.

Answering a remote child (`AnswerSessionQuestion` / `AnswerSessionPermission`) and typing a turn
into one (`SendSessionMessage`) already routed; they are covered by
`cross_host_answer_wait_acceptance` and `project_home_turn_control_acceptance` and unchanged here.

### Session lists (n6)

- `ListSessionsRequest.include_peers` (default false — what a peer is asked, so a listing goes one
  hop) fans out to every other eligible daemon concurrently and returns their own rows, each naming
  its `daemon_instance_id`, with the hosts that did not answer in `unreachable_hosts`. The fan-out
  waits at most 3 s in all (the drawer polls every 2 s; the forward's own deadline is 30 s): a
  slower peer is named unreachable for that poll.
- `ListProjectSessions` rows gain `host_label`, the roster's label for the row's host, resolved by
  the listing host.
- The web session drawer asks with `include_peers`, so inactive sessions on other hosts show,
  merged with live participant rows and labelled by the existing host badge.
  `unreachable_hosts` is on the wire but the drawer does not show it yet.

## Decisions taken (recorded for review)

1. **Push relay over a pull follower.** The sender knows the routing from the child's changeset and
   the message's recipient host; the outbox is the existing durable log, so no second queue.
2. **`ExecuteTool` forward over new typed RPCs.** Reuses the exact path the target tools use and
   avoids six `ExecToolService` implementors; accepted by the operator on condition the names are
   never tools (above), with tests.
3. **Peer-trusted, transport-gated relay calls over carrying session tokens.** A background relay
   holds no token; the receiver checks the anchor session instead.
4. **A relay gap's `lost` is an upper bound.** Once evicted, a record's destinations are unknown.

## Not done here

- A target child's turn-end report to an orchestrator on another host
  (`post_remote_message_to_orchestrator`) still refuses a cross-host orchestrator: targets are n4's
  area.
- The project's main session as a recipient on another host is reached only when the sender's own
  project registry names it, as before.

## Tests

- `packages/tddy-daemon-rpc/tests/cross_host_watch_acceptance.rs` — two daemons in process over the
  in-memory peer forwarder (no LiveKit): messages both ways (including `reply_to`), link down then
  up (delivered once), unknown host refused, a remote sibling refused as on one host, a remote
  child's message, question and end through the watch naming the host, the idle wake, a followed
  non-child session, eviction reported as lost, the relay names never offered or callable, and both
  session lists.
- `packages/tddy-session-activity/tests/notification_relay_unit.rs` — routing, cursor on
  acknowledgement, retry, restart, one hop, dedupe across a reopen, gap records.
