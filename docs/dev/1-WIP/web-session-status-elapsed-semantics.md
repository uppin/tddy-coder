# Web vs TUI elapsed semantics (session status)

## TUI (`format_status_bar`)

- Elapsed is **`goal_start_time.elapsed()`** — an in-memory `Instant` from when the current workflow step started in the running `tddy-coder` process.

## Web / daemon (`ListSessions` enrichment)

- Elapsed is **`format_elapsed_compact(now - step_start)`** where **`step_start`** is parsed from **`changeset.yaml`**: the **`at`** timestamp of the **last** `state.history` entry whose `state` matches `state.current`, or else `state.updated_at`.
- Therefore the web shows **persisted** wall-clock duration since the last recorded transition, not the in-process `Instant`.

## QA comparison

- When the workflow has **persisted** the latest state to `changeset.yaml`, web and TUI **should align** on goal, state, agent, model, and a **similar** elapsed string (same formatting rules in `tddy_core::format_elapsed_compact` and TUI `format_elapsed`).
- If the live process has **not yet written** `changeset.yaml`, the web may show an **older** elapsed or placeholders until the next `ListSessions` poll picks up new disk state.
