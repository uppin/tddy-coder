# Activity log: user prompts, agent streaming, and event channels

**Type**: Technical product behavior (TUI and remote clients)  
**Status**: Active  
**Updated**: 2026-03-29

## Summary

The presenter records user-authored feature and inbox prompts in the scrollable activity log with stable textual prefixes. Workflow agent output appears incrementally in the activity log as chunks arrive, including text before the first line break. Streaming agent text for subscribers uses a single high-volume channel per chunk policy so the same agent chunk is not represented twice as both `ActivityLogged` and raw stream events.

## User-visible activity lines

- **Feature submission**: Non-empty `SubmitFeatureInput` produces an activity line beginning with **`User: `** followed by the submitted text. The line is recorded in `PresenterState::activity_log` and emitted as **`PresenterEvent::ActivityLogged`**.
- **Queued inbox prompt**: Non-empty `QueuePrompt` produces a line beginning with **`Queued: `** followed by the text, with the same logging and broadcast behavior.

Formatting lives in **`tddy_core::presenter::activity_prompt_log`**: stable prefix constants and small formatter functions keep the contract testable.

## Agent output: activity log and broadcasts

- **Incremental tail**: While the backend emits a partial line (no `\n` yet), the last **`ActivityKind::AgentOutput`** row in `activity_log` holds the growing tail. Completed lines flush when a newline arrives; the buffer tracks the current incomplete segment separately.
- **Streaming channel**: Each **`WorkflowEvent::AgentOutput`** chunk is broadcast as **`PresenterEvent::AgentOutput`**. This is the channel remote clients and views use for live agent text. Routine workflow chunks do not also trigger **`PresenterEvent::ActivityLogged`** for the same streaming content, so subscribers do not see duplicate full-line content across both event kinds for standard agent streaming.
- **Interrupt / tool paths**: When the presenter flushes a partial agent buffer (for example around tool **Ask**), the flush path may emit **`ActivityLogged`** for that agent line so subscribers that rely on structured activity entries still see a final line where applicable.

## Subscriber expectations

- Components that render **only** **`ActivityLogged`** for assistant text must also handle **`PresenterEvent::AgentOutput`** to show live workflow agent output.
- **`tddy-service`** maps **`PresenterEvent::ActivityLogged`** and **`PresenterEvent::AgentOutput`** to distinct **`ServerMessage`** variants; consumers combine or subscribe per their UI model.

## Verification

- **Unit**: `activity_prompt_log` prefix tests; `agent_activity` tail and channel policy; `presenter_impl` tests for partial visibility and single authoritative channel per completed line.
- **Integration**: `tddy-coder` **`presenter_integration`** tests for user and queued prompt lines and broadcasts.

## Related documentation

- [Coder overview](1-OVERVIEW.md) — product surface  
- [`packages/tddy-core/docs/architecture.md`](../../../packages/tddy-core/docs/architecture.md) — presenter module layout  
