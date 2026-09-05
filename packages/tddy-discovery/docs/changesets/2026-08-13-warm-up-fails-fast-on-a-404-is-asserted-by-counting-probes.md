# 2026-08-13 — "warm-up fails fast on a 404" is asserted by counting probes, not by the clock

**Type:** Fix

`specialized_agent_warmup_acceptance` checked that a definitive `404` returned within the 500 ms budget; under load one *correct* probe (server start-up, connect, request, response) took 1.076 s, so a busy machine was reported as a broken fail-fast rule. It now asserts `received_requests().len() == 1`, which is what "gave up after one look" actually means and is load-independent. Red-first: mounting a `503` instead makes it fail `left: 23, right: 1`, so the assertion is not vacuous. Production `warmup` unchanged. PR [#385](https://github.com/uppin/tddy-coder/pull/385). (tddy-discovery)
