# 2026-11-18 — An idle session waits for input, and a message to it is delivered

A session that finished its turn and sits at its prompt is reported as waiting for input with
nothing to answer, rather than as waiting on a permission with no request id. `SendSessionMessage`
delivers a turn to it, and an orchestrator watching it is woken as idle. A message is still refused
while a permission prompt or question with a `request_id` is pending, naming the tool that answers it.
