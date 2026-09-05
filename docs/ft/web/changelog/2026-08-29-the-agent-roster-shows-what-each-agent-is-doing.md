# 2026-08-29 — The Agent roster shows what each agent is doing

- **Every row in the Agent roster now carries a status badge** — idle, running, executing tool, waiting for input, connecting or error — updated live, so a dispatched agent is distinguishable from an idle one without prompting it.
- **An agent nothing is known about reads "unknown", never "idle".** "Idle" reads as "free, ready for work", which is a different claim from "nobody here knows" — and the badge is always shown, because a row with no badge and a row whose daemon has nothing to say look identical otherwise.
- **Each row shows what the agent was last seen doing**, as "<what it did> · 4m ago", and only when there is something to show — a blank line reserved for an agent with no history reads as a row that lost one.
- **That timestamp ages on its own**, ticking once a minute: an idle agent produces no updates, so a line that only aged when one arrived would read "just now" for the rest of the session.
