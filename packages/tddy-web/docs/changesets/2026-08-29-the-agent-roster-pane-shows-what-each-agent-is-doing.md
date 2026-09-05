# 2026-08-29 — the Agent roster pane shows what each agent is doing

**Type:** Feature

each row gains a status badge (`data-agent-status`, always rendered: a row with no badge and a row whose daemon has nothing to say look identical otherwise) and a last-activity line rendered only when there is one. `UNSPECIFIED` reads "unknown", never "idle" — an operator reads "idle" as "free, ready for work", a different claim from "nobody here knows". `lastActivityText` formats "<summary> · 4m ago" against a `now` the pane ticks once a minute, because an idle agent produces no frames and a line that only aged on a frame would read "just now" for the rest of the session; a stamp in the future reads "just now" rather than a negative age, since two hosts' clocks disagree by seconds routinely. `useSessionAgentRoster` needed no change — it assigns on every frame and never deduped by `rev`.
