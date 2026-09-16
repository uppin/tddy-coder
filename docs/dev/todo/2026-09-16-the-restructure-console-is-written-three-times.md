# 2026-09-16 — The restructure console is written three times, because the renderer is private

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M5

The same output shapes — findings, per-operation lines, the run summary, a verify comparison — are
stated in three places:

| Where | Why it exists |
|---|---|
| `packages/tddy-code-restructuring/src/restructure_cli.rs` | the cold CLI, and the only module in that crate permitted to print |
| `packages/tddy-tools/src/index_console.rs` | the warm client, rendering events the daemon streamed |
| `packages/tddy-index-daemon/src/render.rs` | the daemon binary's own single-shot console |

`restructure_cli`'s `report`, `report_findings`, `report_comparison` and `install_console` are all
**private**, and that module's entire public surface is `run(RestructureArgs)` — which owns the whole
run *including spawning the language server*, the one thing a client of the daemon must not do. So
there is no reachable renderer, and each front end restates the wording.

## What holds the line meanwhile

`restructure_runs_against_the_warm_daemon_when_the_socket_variable_is_set`
(`packages/tddy-tools/tests/index_daemon_client_acceptance.rs`) asserts the **whole stdout vector**
of a `check --budget` run against the literal lines `restructure_cli_acceptance.rs` pins for the cold
path, and a sibling test does the same for the stamped stderr. Drift fails a test rather than going
unnoticed — which it already proved, catching the stream-rule divergence the moment #500 was merged.

Two smaller duplications ride along, each commented at both sites: items normalisation
(`normalised_items` / `normalised`) and the UTF-8 path refusal.

## What closing it would take

A published renderer in `tddy-code-restructuring` over the result vocabulary the same changeset
introduced — `Outcome`, `Finding`, `PlanProgress`, `RunSummary`, `Comparison`. It has to print, so it
belongs in `restructure_cli.rs` to keep
`only_the_command_line_front_end_writes_to_standard_output` true; alternatively it returns lines and
each front end writes them, which is cleaner and lets the daemon reuse it too. `TODO` at
`packages/tddy-tools/src/index_console.rs:15`.
