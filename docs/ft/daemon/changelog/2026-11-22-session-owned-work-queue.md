# 2026-11-22 — Orchestrators own a work queue from their start

An orchestrator-role session — main and every other orchestrator — owns an empty work queue from its
start; project stack records are deleted at daemon start. See
[Project concept § Orchestrator sessions](../project-concept.md#orchestrator-sessions) (`#workqueue` 5/14, PR #1006).
