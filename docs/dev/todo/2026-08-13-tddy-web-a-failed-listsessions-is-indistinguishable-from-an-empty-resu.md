# 2026-08-13 — tddy-web — a failed `ListSessions` is indistinguishable from an empty result in the new-session form

**Category:** Future enhancement
**Source:** pr-stack-base-session changeset, 2026-08-13

`CreateSessionPane`'s mount effect fetches sessions best-effort and swallows the failure. That was
tolerable when the list only fed the optional "PR stack parent" picker; it now also decides whether the
"Base the stack on" picker has any options, so an operator who came specifically to seed a stack sees
only "None (agent plans the stack)" and cannot tell "no eligible sessions" from "the fetch failed" —
the likely outcome being an unseeded orchestrator created by accident. Keep the fetch non-fatal, but
record the failure and say so in the picker's help text.

The same effect also never refetches when the in-form daemon selector changes (its dependency list
omits the host), so the offered sessions can belong to a different host than the one that will run the
session.
