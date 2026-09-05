# 2026-07-26 — Agent Activity: a tool call with no body says so

- The detail dialog's **Output section no longer disappears** when a tool call has no output — it says so, and says it differently for a call that is **still running** ("No output yet") than for one that has **finished** without producing output. Previously both looked like the section did not exist. See [agent-activity-pane.md § Rendering an unanswered lookup](../agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26).
- A tool call with **no input** now says so too, instead of rendering an **empty highlighted block** that read as "the input was empty".
- While a body is being fetched, each JSON block is replaced by a **skeleton** in the shape of the content it stands in for, rather than a single "Loading…" line.
- Failures are now worded by cause — lookup failed, the host knows no such tool call, or the entry carries no tool call id — instead of showing the **raw transport error message** to the operator. An entry with no tool call id no longer triggers a pointless lookup at all.
- A **still-running** tool call's body is no longer cached, so its output appears when it lands; previously the first (empty) body was cached for the life of the page. Completed calls are still served from cache instantly, now with no loading flash.
