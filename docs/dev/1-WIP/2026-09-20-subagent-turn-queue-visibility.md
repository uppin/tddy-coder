# Changeset: subagent-turn-queue-visibility

PRD (target): [`docs/ft/coder/managed-codebase-subagents.md`](../../ft/coder/managed-codebase-subagents.md)
PR: [#521](https://github.com/uppin/tddy-coder/pull/521)

## Problem

A conversation runs **one turn at a time** — `SubagentConversation::session` is an
`Arc<tokio::sync::Mutex<Box<dyn SubagentSession>>>`, and `run_turn` holds it for the whole turn.
But `subagent_prompt` accepts a prompt on a conversation that is already mid-turn: it spawns
another task, registers another `responseId`, and answers `{responseId, pending: true}` — the same
shape it returns for the *first* prompt. Nothing says the conversation was busy.

Observed live. Asked to fill a subagent's context window, the main agent read `pending: true` as
"started, running in parallel" and fired **ten prompts** at one conversation, then spent the rest of
the session awaiting ten ids in scattered order — 94 `subagent_prompt` calls and 97
`subagent_await` calls in a single transcript. Its own conclusion:

> *"Not queueing properly and all the pending requests are blocking each other."*

Half right, and that is the problem: the requests **were** blocking each other, by design, and it
had no way to see that. It read a queue as a bug.

## State B

`{responseId, pending: true}` gains two fields, and every surface that reports on a turn reports
them:

| Field | Meaning |
|---|---|
| `queuePosition` | how many turns on this conversation are ahead of this one — `0` means it is running now |
| `queueSize` | how many turns are outstanding on this conversation in total, this one included |

- **`subagent_prompt`** returns them with the deferral, so a caller learns *at the moment it
  queues* that it is third in line, rather than inferring it from silence.
- **`subagent_await`** returns the **current** position when the turn is still pending — the number
  that moves as the queue drains, which is the one a caller polling for progress needs.
- **`subagent_list`** reports `queued` per conversation, beside the token accounting it already
  carries.

The tool descriptions say plainly that a conversation runs one turn at a time and a prompt to a
busy conversation waits its turn. That sentence alone would have prevented the incident; the
numbers are what let a caller act on it.

## Ordering: what the number means, and what it does not

Position is **arrival order** — the order `PendingTurns::start` was called, which is the order the
prompts were accepted, recorded as a monotonic sequence on each entry.

Execution order is *mutex acquisition* order. `tokio::sync::Mutex` is fair, so the two normally
agree, but the spawned tasks race to make their first `lock()` call and the scheduler decides who
gets there first. So a reported position is an accurate statement about **how many prompts were
accepted before this one**, not a guarantee about completion order. The field names and the tool
description say "waiting" rather than promising a strict sequence, because overstating it would be
worse than saying nothing.

## Boundaries

- No explicit queue replacing the mutex. The serialisation is correct; only its visibility is
  missing, and a real queue would be a larger change for no behavioural gain.
- No refusal of a prompt to a busy conversation. Queueing is legitimate — a caller may well want to
  line work up; it just has to know that is what it is doing.
- No change to `graceMs`, `timeoutMs`, or the deferral mechanism itself.

## Acceptance criteria

1. A prompt to an idle conversation reports `queuePosition: 0`.
2. A prompt to a conversation with a turn in flight reports `queuePosition: 1`, and the next `2`.
3. `queueSize` counts every outstanding turn on that conversation, and does not count turns on
   other conversations.
4. `subagent_await` on a still-pending turn reports its **current** position, which decreases as
   earlier turns finish.
5. A finished turn is not counted in either number.
6. `subagent_list` reports `queued` per conversation.

## TODO

- [x] Changeset
- [ ] Failing tests
- [ ] Implementation
