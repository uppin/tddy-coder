# Activity log: user prompts, agent streaming, and event channels

**Type**: Technical product behavior (TUI and remote clients)  
**Status**: Active  
**Updated**: 2026-04-04

## Summary

The presenter records user-authored feature and inbox prompts in the scrollable activity log. Submitted feature text is stored as plain text (no prefix). Queued inbox prompts use a **`Queued: `** prefix. Both use **`ActivityKind::UserPrompt`** so views can style them consistently. Workflow agent output appears incrementally in the activity log as chunks arrive, including text before the first line break. Streaming agent text for subscribers uses a single high-volume channel per chunk policy so the same agent chunk is not represented twice as both `ActivityLogged` and raw stream events.

## User-visible activity lines

- **Feature submission**: Non-empty `SubmitFeatureInput` stores the submitted text in `PresenterState::activity_log` and emits **`PresenterEvent::ActivityLogged`** with **`ActivityKind::UserPrompt`** (plain text; no `User: ` prefix).
- **Queued inbox prompt**: Non-empty `QueuePrompt` produces a line beginning with **`Queued: `** followed by the text, with **`ActivityKind::UserPrompt`** and the same logging and broadcast behavior.

Formatting lives in **`tddy_core::presenter::activity_prompt_log`**: **`format_user_prompt_line`** returns the submitted string; **`format_queued_prompt_line`** applies the queued prefix.

## TUI presentation (local and Virtual Tui)

In **`tddy-tui`** `render::draw`, entries with **`ActivityKind::UserPrompt`** render as a fixed **three-row** block inside the activity pane:

- **Margins**: One blank line above and below the block; one column of margin on the left and right of the styled area (inset from the pane edges).
- **Rows**: The first row of the block is empty (padded panel only). Wrapped text occupies the second and third rows; overflow beyond two text rows is merged into the third row with an ellipsis (**`…`**) when needed.
- **Style**: Panel fill **`Rgb(85, 85, 85)`**; foreground **`Rgb(255, 255, 255)`** with **bold** weight.

## Agent output: activity log and broadcasts

- **Incremental tail**: While the backend emits a partial line (no `\n` yet), the last **`ActivityKind::AgentOutput`** row in `activity_log` holds the growing tail. Completed lines flush when a newline arrives; the buffer tracks the current incomplete segment separately.
- **Streaming channel**: Each **`WorkflowEvent::AgentOutput`** chunk is broadcast as **`PresenterEvent::AgentOutput`**. This is the channel remote clients and views use for live agent text. Routine workflow chunks do not also trigger **`PresenterEvent::ActivityLogged`** for the same streaming content, so subscribers do not see duplicate full-line content across both event kinds for standard agent streaming.
- **Interrupt / tool paths**: When the presenter flushes a partial agent buffer (for example around tool **Ask**), the flush path may emit **`ActivityLogged`** for that agent line so subscribers that rely on structured activity entries still see a final line where applicable.

## Subscriber expectations

- Components that render **only** **`ActivityLogged`** for assistant text must also handle **`PresenterEvent::AgentOutput`** to show live workflow agent output.
- **`tddy-service`** maps **`PresenterEvent::ActivityLogged`** and **`PresenterEvent::AgentOutput`** to distinct **`ServerMessage`** variants; activity kinds include **`UserPrompt`** for user-submitted and queued prompt lines. Consumers combine or subscribe per their UI model.

## Verification

- **Unit**: `activity_prompt_log` formatters; `agent_activity` tail and channel policy; `presenter_impl` tests for partial visibility and single authoritative channel per completed line; **`tddy-tui`** `render` tests for user-prompt row layout.
- **Integration**: `tddy-coder` **`presenter_integration`** tests for user and queued prompt lines and broadcasts.

## Related documentation

- [Coder overview](1-OVERVIEW.md) — product surface  
- [`packages/tddy-core/docs/architecture.md`](../../../packages/tddy-core/docs/architecture.md) — presenter module layout  
- [`packages/tddy-tui/docs/architecture.md`](../../../packages/tddy-tui/docs/architecture.md) — TUI activity log rendering  
