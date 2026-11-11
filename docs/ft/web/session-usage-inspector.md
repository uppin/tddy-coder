# Session Usage — Real-time Token Usage in the Inspector

**Component:** `SessionInspectorDrawer` → **Usage** tab (`packages/tddy-session-web/src/`)
**Updated:** 2026-11-11
**Status:** Implemented

## Overview

The Session Inspector's **Usage** tab shows a live, per-conversation token breakdown for the selected
session: the main agent, each subagent nested under the conversation that spawned it, and a TOTAL
row. Every session the daemon runs has it: host terminal (PTY) sessions, jailed terminal sessions and
workflow sessions, live or ended. Values update in near-real-time while the session runs.

What a conversation row holds and how an agent's files are read is
[Session Token Accounting](../coder/session-token-accounting.md): each coding agent declares a
`coding_agent.usage` binding in its spec, and one engine evaluates it.

## Where the usage comes from — the daemon

**Every launch records a usage context.** The host PTY, jailed and workflow launches, and every resume
and relaunch, write `<session_dir>/usage-context.json` next to `.session.yaml`:

| Field | Value |
|---|---|
| `spec_id` | the agent spec the session runs — whose `usage:` binding applies |
| `agent_home` | the agent's HOME as a host path: the jail's home for a jailed session (`<tddyhome>/homes/<key>` or the session's own), the OS user's home for a host or workflow session |
| `conversation_id` | the main conversation's id, when the agent's conversation has one the daemon knows (the session id for `daemon_session_id`, a minted id, or the resume token) |
| `worktree` | the session's working tree |
| `model` | the model the session runs — the main row's model when the agent's files name none |

A resume or relaunch rewrites it with the context it recomputed, so a jail relaunched with a different
home is read from the new one.

**The daemon evaluates usage while a client watches.** `ActivityService.StreamSessionUsage` serves it:

```protobuf
rpc StreamSessionUsage(StreamSessionUsageRequest) returns (stream SessionUsageSnapshot);

message StreamSessionUsageRequest { string session_token = 1; string session_id = 2; string daemon_instance_id = 3; }
message SessionUsageSnapshot { repeated UsageConversation conversations = 1; }
message UsageConversation {
  string agent = 1; string id = 2; string model = 3;
  uint64 input_tokens = 4; uint64 cache_read_tokens = 5; uint64 cache_write_tokens = 6;
  uint64 output_tokens = 7; uint64 reasoning_tokens = 8; uint64 total_tokens = 9;
  uint32 turns = 10; optional double cost_usd = 11; optional string parent_id = 12;
}
```

- The **first subscriber** of a session starts one poller for it; later subscribers join it. Every
  2 s the poller reads the usage context, takes the spec's binding from the daemon's coding-agent
  registry, and evaluates it, adding the tddy subagent rows from `egress/accounting.json`.
- A subscriber gets the **current snapshot at once**, then the whole snapshot again **each time it
  changes** — never a delta, never an unchanged repeat. The client replaces what it holds.
- The poller **stops when its last subscriber leaves**; nothing is evaluated for a session nobody is
  watching.
- **Live and ended sessions are alike**: an ended session is evaluated from the same files, and its
  snapshot simply stops changing. A session whose launch predates the usage context streams only its
  tddy subagent rows.
- A request addressed at another daemon (`daemon_instance_id`) is refused, as `StreamSessionActivity`
  is: the web opens the stream on the host that owns the session.

The stream is authorised by the web session token, like the other per-session Inspector streams. No
LiveKit room, no presenter and no `tddy-coder` process takes part.

## Layout

The tab reaches the daemon the way every other Inspector tab does — the session host's
`ActivityService` client — so a session with no LiveKit room has usage too.

```
agent          model            in      cache read  cache write  out    reasoning  total    cost    turns
claude         claude-opus-4-8  12      332,059     43,803       2,057  0          377,931  $1.23   6
  Explore      claude-haiku     4,100   0           0            820    0          4,920            2
    Plan       claude-haiku     900     0           0            120    0          1,020            1
fastcontext    ollama           5       0           0            1      0          6                1
TOTAL                           5,017   332,059     43,803       2,998  0          383,877  $1.23*
```

- **Columns**: In · Cache read · Cache write · Out · Reasoning · Total · Cost · Turns. The counters are
  disjoint, so Total is their sum.
- **Cost** shows `$x.xx` when the source reported one and is blank otherwise. The TOTAL cost sums the
  rows that have one; when some rows have none it carries a `*` and the title "partial: some
  conversations did not report a cost". With no row reporting a cost the TOTAL cost is blank.
- **Rows are a tree.** A conversation with a `parent_id` renders after its parent, indented by its
  depth (any depth); siblings keep snapshot order. A child whose parent is not in the snapshot is a
  top-level row.
- Before any snapshot arrives the tab shows its empty state. A stream that fails keeps the last
  snapshot on screen and says why beside it ("Usage unavailable: …"); with no daemon client for the
  session's host yet, there is no stream to fail and the tab stays in its empty state.

## Frontend

- `useSessionUsage(client, sessionToken, sessionId)` — opens `ActivityService.StreamSessionUsage`
  on the session host's client, keeps the latest snapshot, aborts on unmount or when the session
  changes.
- `sessionUsage.ts` — `ConversationUsage`, `buildUsageTree` (rows with their `depth`) and
  `usageTotals` (every counter, the summed cost and `costPartial`).
- `SessionUsageTab` — the table, the tree indentation, the cost cells and the TOTAL row, using
  `formatTokens`. It owns the subscription, so the stream is open only while the tab is shown.
- Test ids: `sessions-usage-row-{agent,model,input,cache-read,cache-write,output,reasoning,total,cost,turns}-<id>`
  (the row itself is `sessions-usage-row-<id>`, carrying `data-depth`) and
  `sessions-usage-total-{input,cache-read,cache-write,output,reasoning,total,cost}`.

## Scope

- **In scope:** every daemon-run session's usage, live and ended; the cache, reasoning and cost
  columns; the conversation tree.
- **Out of scope:** USD cost estimation (a cost is shown only when the agent's files report one);
  persisted usage history beyond what the agents' own files hold; charts.
- **Known gaps:**
  - a workflow session spawned without an agent runs `tddy-coder`'s default agent but records no
    usage context, so it shows only its tddy subagent rows —
    [todo](../../dev/todo/2026-11-11-workflow-spawn-without-agent-records-no-usage-context.md);
  - an agent whose conversation has no id the daemon knows has no main row (the binding engine needs
    a conversation id);
  - `tddy-coder`'s session participant, which serves `ActivityService` for LiveKit-routed sessions,
    does not serve `StreamSessionUsage`; the web always opens it on the daemon.
