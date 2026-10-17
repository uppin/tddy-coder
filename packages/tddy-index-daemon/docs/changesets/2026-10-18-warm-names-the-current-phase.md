# 2026-10-18 — `Warm` names the current phase
**Type:** Fix

`IndexProgress.phase`/`percentage` on the `Warm` stream are the phase the server reported on most
recently — named from its `begin` — with the highest percentage that phase reached, read from
`ServerChatter::current()`. They were filled from the furthest percentage of any phase, so a real
warm of this workspace (7m31s) read *Roots Scanned 100%* from its first seconds to its end. `furthest`
keeps the cross-phase figure. This is what `code_navigation.proto` already documented.

Docs: [code-index-service.md](../code-index-service.md). Tests:
`warm::tests::names_the_phase_running_now_rather_than_the_one_that_reached_the_highest_percentage`;
`code_index_service_acceptance::warming_a_root_forwards_the_servers_phases_and_reports_ready_only_once_it_is_quiescent`
expects the phase from its begin message (0%).
