# 2026-09-27 — A subagent turn has no wall-clock deadline, so a slow provider hangs it as surely as a runaway one did

**Category:** Future enhancement (deferred from the subagent input-validation and output-bounds change)
**Source:** Session `01a0e200-37a6-7281-b925-b85314cdf76f`, the same investigation that produced
`SUBAGENT_MAX_OUTPUT_TOKENS` (see `packages/tddy-discovery/tests/subagent_generation_cap_red.rs`).

## Why it was deferred

The change in flight bounds the **tokens** a turn may generate, because that is what actually
wedged session 01a0e200: `llama-server` reported `n_gen = 19790` and climbing at 24 t/s after
thirteen minutes, with no stop token. A `max_tokens` on the request ends that case.

It does **not** end the other one. A provider that accepts the connection and then generates
slowly — or stalls mid-stream — still holds the turn open indefinitely, and the token cap never
fires because the tokens never arrive. `OpenAiClient::timeouts` exists
(`packages/tddy-discovery/src/openai.rs`) and the subagent path does not use it: `OpenAiClient::new`
builds a deadline-free `reqwest::Client`, which its own doc comment warns about.

Left out because it is a different mechanism with a different failure mode — a deadline that is
too short turns a legitimately long search into a spurious failure, so picking the budget needs its
own thought, and folding that into a change already spanning five surfaces would have made both
harder to review.

## What would close it

- Give the subagent's `OpenAiClient` a request deadline via the existing `timeouts` builder, and
  decide the budget deliberately — a tool-loop turn and a synthesis turn are not the same call.
- Surface an expired deadline as a stop reason the caller can act on, the way
  `StopReason::MaxTokens` will surface a token-capped turn, rather than as a transport error.
- Note that `subagent_await`'s `timeoutMs` does **not** cover this: it bounds how long the *main
  agent* waits, and explicitly cancels nothing (`SUBAGENT_PROMPT_GRACE`,
  `packages/tddy-tools/src/server.rs`). A turn abandoned by its caller keeps running.

## Deployment note, not repo code

Two settings in the Modelfile for `fastcontext-tools-32k:latest` made the runaway unrecoverable
rather than merely long, and neither lives in this repository:

- `PARAMETER repeat_penalty 1` — repetition penalty disabled, so degenerate repetition is
  unpenalized.
- Ollama runs `llama-server` with `--context-shift` and `n_keep = 4`. On overflow it discarded
  16,381 tokens and kept four, which throws away the system prompt and the tool schemas — after
  which the model cannot emit a valid tool call or a stop sequence at all.

Recorded here because the next person reading the token cap will reasonably ask why a 32k model ran
past its window without erroring. The answer is not in our code.
