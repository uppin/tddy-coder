# Changeset: specialized-agent-context-handoff

PRD (target): [`docs/ft/coder/specialized-subagents.md`](../../ft/coder/specialized-subagents.md)
PR: [#521](https://github.com/uppin/tddy-coder/pull/521)
Sibling changeset in the same PR: [`2026-09-20-sandboxed-codebase-managed-workflow.md`](2026-09-20-sandboxed-codebase-managed-workflow.md)

## Problem

A specialized agent's message history only ever grows. `SpecializedSubagentSession::messages` is
pushed to at seven sites in `packages/tddy-discovery/src/subagent.rs` and never drained, truncated
or cleared; the sole bound on growth is `DEFAULT_READ_LINE_CAP = 200`, which caps **one** `READ`
and says nothing about accumulating fifty of them.

So the window fills, and when it does the provider's refusal is propagated verbatim:

```rust
let response = client.complete(request).await.map_err(|e| {
    SubagentError(format!("{error_context}: {e}"))
})?;                       // out of run_one_turn → out of prompt → subagent_error_json
```

Three consequences, none of them handled:

1. **The turn's work is lost.** Turn exhaustion has a soft landing (`run_synthesis_turn`, stop
   reason `MaxTurnRequests`); context exhaustion has none.
2. **The conversation is wedged permanently.** `messages` is conversation-scoped and nothing
   removes the oversized history, so every later `subagent_prompt` re-sends it and fails
   identically. Only `subagent_cancel` clears it.
3. **It is illegible.** The caller sees a generic provider string, indistinguishable from a network
   failure, so it has no reason to do anything different.

This bites hardest exactly where specialized agents are most useful. The shipped example runs them
on Ollama at 32k (`sandbox-config.example.yaml`: `fastcontext-tools-32k`, `num_ctx 32768`), and an
exploration agent's whole job is reading code.

## State B

### A context refusal is recognised and named

`StopReason::ContextExhausted` joins `EndTurn` / `MaxTurnRequests` / `Cancelled`. A provider error
that is a context-length refusal produces a **`PromptOutcome`, not an `Err`** — the same soft
landing `MaxTurnRequests` already gets, for the same reason: the caller should receive what was
gathered rather than a failure string.

Detection is over the error text, because that is the whole surface `OpenAiClient::complete`
offers (`Box<dyn Error>` wrapping `"OpenAI API error {status}: {body}"`). That is fragile by
nature, so the predicate is narrow, named, and pinned by tests carrying the **real** phrasings from
each provider rather than an invented one. A refusal it does not recognise keeps today's behaviour
exactly — an error — so a mis-detection can only ever fail to help, never mislabel a network blip
as a full context.

### The brief is compacted so the work is not redone

The outcome carries a handoff brief the calling agent can hand to a fresh conversation. It cannot
be model-generated — the context that would summarise it is the context that is full — so it is
built **mechanically** from the history, on one rule:

> **Drop the payloads. Keep the conclusions and the index of what was already examined.**

| Kept | Why |
|---|---|
| The agent's goal — its system prompt intent and the original user prompt, verbatim | A new conversation starts with no idea what it was for |
| Every assistant **text** block | This is where the findings are. Discarding them is what makes an agent redo the work |
| Every tool call as `name` + arguments — `READ foo.rs:1-200`, `GREP "fn spawn"` | The explicit *already examined* list. This is what stops the new agent re-reading the same files |
| The last assistant turn | Where it had got to |

| Dropped | Why |
|---|---|
| Tool **results** — file contents, grep output | The bulk, and what filled the window. A finding derived from a file is worth keeping; the file is not |

The brief ends with instructions addressed to the calling agent: open a **new** conversation with
this same specialized agent, pass the brief as its first prompt, and continue from the open
question — explicitly **not** re-examining anything on the already-examined list.

### The main agent can see how full the context is — before it fills

`subagent_list` already reports `{agent, id, model, inputTokens, outputTokens, totalTokens, turns}`,
but that is cumulative **spend**, not occupancy: every turn re-sends the whole history, so
`inputTokens` is the sum of growing prefixes and over-counts the window several times over. A
caller watching it cannot tell a conversation that is nearly full from one that has simply run many
cheap turns.

What answers the question is the **most recent turn's prompt tokens** — that is, by definition, what
the history currently costs to send. The session tracks it and `subagent_list` reports it as
`contextTokens`, beside the spend it already carries. A main agent can then wind a conversation down
on its own terms rather than discovering the ceiling by hitting it.

### The aborted conversation's tail rides in the failed response

The brief is lossy on purpose — it drops the payloads that filled the window. Sometimes the useful
thing is the opposite: the last few exchanges exactly as they happened, to see precisely where the
agent was when it stopped.

That tail travels **in the `ContextExhausted` outcome itself**, beside the brief, rather than behind
a second call. A caller that has just been told its conversation is unusable should not have to ask
a follow-up question to find out where it stopped — and a `subagent_tail` tool would also have to be
added to the jailed agent's allowlist
(`packages/tddy-sandbox-recipes/src/claude_cli.rs`), which is surface spent for a round trip nobody
wants. One response carries everything needed to rebuild the conversation.

Each tail message is truncated to a bound, so reading the tail cannot itself be the thing that fills
the caller's context.

`tail(n)` stays a method on the session — it is how the outcome builds that section — but it is not
an MCP tool.

### Honest limit

If the model narrated little and mostly called tools, the kept conclusions are thin. The
already-examined index still prevents repeated reads, which is the expensive half, but the new
conversation may have to re-derive reasoning. Recorded here rather than papered over: a faithful
mechanical compaction cannot invent findings the transcript does not contain.

## Boundaries

- No compaction *during* a conversation — this changeset makes exhaustion survivable, it does not
  prevent it. Sliding-window or summarising compaction is a larger change with its own trade-offs.
- The daemon does not auto-open the replacement conversation. The calling agent decides, because it
  is the one holding the surrounding task.
- No change to `max_turns` or to the existing synthesis landing.

## Acceptance criteria

1. A context-length refusal yields `StopReason::ContextExhausted` rather than an error, for each
   provider phrasing tested.
2. An unrelated provider failure still yields an error — detection does not over-reach.
3. The brief names the goal, lists what was already examined, and carries the assistant's findings.
4. The brief omits tool-result bodies.
5. The brief instructs the caller to open a new conversation and not to repeat the examined work.
6. `subagent_list` reports `contextTokens` — the current occupancy, distinct from cumulative spend.
7. The `ContextExhausted` outcome carries the conversation's tail verbatim and bounded, so the
   caller needs no second call to see where it stopped.

## TODO

- [x] Changeset
- [x] Failing tests
- [x] Implementation
