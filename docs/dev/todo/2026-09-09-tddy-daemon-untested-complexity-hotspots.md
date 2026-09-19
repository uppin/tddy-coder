# 2026-09-09 — First CRAP results for `tddy-daemon`: the untested-complexity hotspots

**Category:** Missing coverage

> **Scope note (2026-09-19, `#carve` 4/10).** These figures were captured while `tddy-daemon` held
> **141** test binaries, **119** of which exercised other crates. Those 119 now live in the crates
> they test, so a re-run of `analyze coverage` scoped to `tddy-daemon` will capture a small fraction
> of the 2,159 tests counted here and the by-file percentages will not be comparable.
>
> **Nothing here is fixed and nothing is claimed.** The hotspots are unchanged: `telegram_bot.rs`,
> `connection_service.rs`, `session_agent_clone.rs` and `telegram_session_control.rs` are
> `tddy-session-lifecycle`'s, and `livekit_peer_discovery.rs` is `tddy-daemon-livekit`'s — all were
> already there, since `#carve` 4/10 moves no `src/` file. What changed is which crate's test run
> covers them, and the next measurement should be taken per owning crate rather than against
> `tddy-daemon`.
>
> The caveat below about `session_agent_remote_acceptance.rs` and
> `remote_managed_worktree_cross_host_acceptance.rs` still applies as written: both stay in
> `tddy-daemon`, because both exercise its composition root.
**Source:** `analyze-coverage-export-and-harness-selection` (#466), `analyze coverage` + `report`

The first CRAP report the repo has been able to produce. 2,159 tests captured, 9,064 instrumented
functions, of which **2,531 (27.9%) are never executed by any test**. Read these as
*complex and never executed*: CRAP treats coverage as a boolean
([see that item](2026-09-09-crap-scores-coverage-as-a-boolean-not-a-ratio.md)), so every score below
is exactly `cx² + cx` and the ranking is complexity among functions no test enters.

| CRAP | Complexity | Location |
|---|---|---|
| 7,832 | 88 | `telegram_bot.rs:368` `telegram_callback_handler` |
| 2,756 | 52 | `telegram_bot.rs:211` `telegram_message_handler` |
| 650 | 25 | `connection_service.rs:7884` `relaunch_sandboxed_runner` |
| 506 | 22 | `connection_service.rs:7606` `split_context_from_codebase_host` |
| 380 | 19 | `session_agent_clone.rs:480` `run_clone_mirror` |
| 306 | 17 | `telegram_session_control.rs:2156` `spawn_telegram_workflow` |

By file, never-executed functions against total:

- **`connection_tonic_adapter.rs` — 114 / 120.** 95% of the file never runs under any test; the
  starkest gap in the crate.
- `connection_service.rs` — 634 / 1,538, in a 21,900-line module.
- `telegram_session_control.rs` — 130 / 329, and 13 of the top-50 CRAP entries.
- `livekit_peer_discovery.rs` — 84 / 174.

`telegram_bot.rs` holds the two worst functions in the crate by a wide margin — complexity 88 and 52,
both entirely untested — which makes it the first target for either tests or decomposition.

**Caveat when reading the by-file numbers:** several `tests/*.rs` files also show as fully
never-executed (`session_agent_remote_acceptance.rs` 99/99, `remote_managed_worktree_cross_host_acceptance.rs`
53/53). Those are the suites broken by
[the `self_arc` wiring gap](2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md) and the
LiveKit container flake — their bodies genuinely never ran. That is baseline breakage surfacing, not
dead production code, and it also means the daemon's true coverage is *better* than these figures
once those 18 tests pass.

**Duplicate tests:** **0 identical signatures** across 2,159 tests, which is a good result. 1,040
subset pairs, but the ≥99% band is mostly the same broken suites aborting in shared setup — high
overlap there means "these barely ran". The actionable ones are the small subsets, e.g.
`relay_config_section_defaults_to_none_when_absent` and `relay_config_section_parses_with_idle_timeout`,
each wholly contained in 39 other tests.

Recorded rather than acted on: `analyze-code-issues` hands off to
[`code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md) and does not restructure.
