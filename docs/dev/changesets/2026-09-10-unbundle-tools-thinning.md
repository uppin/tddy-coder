# 2026-09-10 — `tddy-tools` becomes CLI dispatch, the permission engine and the MCP router

**Type:** Architecture

Node 5 of the `#unbundle` stack ([#474](https://github.com/uppin/tddy-coder/pull/474), 5 of 9, based
on node 4's branch). Twenty-four of `tddy-tools`' twenty-eight modules move to the crates that
already own their domains — **ten that existed, plus one new `tddy-session-tool-client`**. What is
left is the four things the crate exists for: `main.rs`'s argument parsing and dispatch, the clap arg
structs, the Claude Code `--permission-prompt-tool` decision engine, and the `rmcp` `ServerHandler`
that assembles the tool router.

Nothing on the wire moved. No proto changed, no client migrated, the CLI surface is identical and
the twenty-one `TDDY_*` environment variables are read from the same places under the same names.
There is **no PRD** — this is a behaviour-preserving restructure, and the one deliberate exception
to that is stated below.

## What landed

| | |
|---|---|
| `tddy-tools` | **24 → 12 `src` files**, **14 → 6 public modules**, **11,501 → 5,594** non-blank production lines, **38 → 28** `[dependencies]` (14 dropped, 4 added — a net 10). No `build.rs`. No import cycles. Zero LiveKit paths |
| `tddy-session-tool-client` *(new)* | `session_tool_client` moved verbatim, 1,005 lines at the crate root: transport detection and tool dispatch over sandbox IPC, daemon HTTP, RPC or LiveKit. `livekit` is a **default-off** feature |
| `tddy-workflow-recipes` | `schema`, `schema_manifest`, `github_pr`; `review_persist` deleted. +`include_dir`, +`jsonschema`, +`tempfile` |
| `tddy-discovery` | `roster` — all five `session_agents` modules, 1,802 lines — and `subagent_runtime`, seam D's conversation table and the turns that outlive their calls. +`tddy-service`, +`tddy-session-tool-client`, +`tddy-rpc`, +`prost`, +`uuid`, +a forwarding `livekit` feature |
| `tddy-terminal-rpc` | `pty_relay`'s four dispatch modes, 766 lines, plus an internal `local_terminal` both relays share. +`tddy-service`, +`reqwest`, +an optional `tddy-livekit` |
| `tddy-tool-engine` | `dynamic_proxy::is_native_tool_denied_in_remote_mode`, and **one exec-tool catalog instead of two**. Dependencies unchanged — no `rmcp`, no `anyhow` |
| `tddy-core` | `toolcall::client` (the client half of a wire whose listener it already hosted), the CLI's request/response shapes, `session_actions::{authoring, session_dir, tool_gate}`, `backend::model_catalog`, `spawn_env::env_non_empty`. +`anyhow`, on an explicit developer decision |
| `tddy-service` | `session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`, beside the `SESSION_AGENTS_TOPIC` already there. Dependencies unchanged — deliberately, it is *below* the tool client |
| `tddy-bsp` | the `build`/`build-list` dispatch; **`plugin_registry` exists once**; `run_build`/`run_build_list` take no relay function pointer, and `RelayFuture`/`ToolcallRelay` are deleted |
| `tddy-code-analysis`, `tddy-code-restructuring`, `tddy-lsp-executor` | their subcommand dispatches; `cli_vector()` deleted |
| `tddy-testing-commons` | repointed at the destination crates, resolving the prod/dev cycle that had to go first |
| `tddy-daemon`, `tddy-sandbox-app`, `tddy-sandbox-darwin` | the `tddy-tools` **dev-dependency is gone**, asserted by a test in each |

Docs: [`json-schema.md`](../../../packages/tddy-workflow-recipes/docs/json-schema.md),
[`tddy-session-tool-client`](../../../packages/tddy-session-tool-client/README.md),
[`tddy-tools`](../../../packages/tddy-tools/README.md),
[roster and subagent runtime](../../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md),
[`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md),
[`tddy-core` architecture](../../../packages/tddy-core/docs/architecture.md).

## Three crates stopped depending on `tddy-tools`, and it is asserted rather than described

`tddy-daemon`, `tddy-sandbox-app` and `tddy-sandbox-darwin` each carried a **dev**-dependency on
`tddy-tools` for exactly three symbols: `session_tool_client::{dispatch_via_sandbox_ipc,
dispatch_session_tool}` and `session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`. The first two are now
`tddy-session-tool-client`, the third `tddy_service::session_agents`, and
`unbundle_tools_dependency_dropped.rs` in each crate walks the manifest and fails if the string
returns.

The assertion is the point. **A dev-dependency survives invisibly** — nothing fails to compile when
one merely stops being used — so a drop that is only described is a drop that silently comes back.

The suites that motivated the move kept calling `dispatch_session_tool` rather than being lowered to
`dispatch_via_sandbox_ipc`, so their coverage of env-driven transport detection is intact. Lowering
them would have deleted that coverage to make a manifest assertion pass.

## Three duplications deleted, and three more found on the way

Each of the planned three existed **because** one copy was locked in a binary crate, so the move is
the moment at which deleting it is both safe and obvious.

- **`build_cli::plugin_registry()`** registered the same five build plugins as
  `tddy_bsp::plugins::plugin_registry`, verbatim. Moving the dispatch into `tddy-bsp` deletes one of
  them and lets `tddy-tools` drop all six `tddy-build*` dependencies.
- **`server::exec_tool_catalog()`** was a hand-copied `RemoteToolDef` clone of
  `tddy_tool_engine::catalog::tool_catalog()`, with matched guard tests in both crates. It is now
  eight lines mapping the real catalog into the MCP shape, and the advertised set is byte-identical
  over the `--mcp` wire. `tddy-tools`' guard test is deleted; **`tddy-daemon`'s is kept**, because it
  guards a pair that has *not* collapsed — this catalog against `tddy_sandbox::
  workspace_exec_tool_names`, the `--allowedTools` a sandboxed `claude` is spawned with.
- **`review_persist.rs`** was 15 lines delegating to
  `tddy_workflow_recipes::review::persist_review_md_to_session_dir`, and is deleted outright.

Three more the plan had not counted:

- **`RawMode` and the terminal-size probe.** `pty_relay.rs` and `tddy-terminal-rpc`'s
  `local_pty_relay.rs` each declared a byte-identical termios guard with the same `24 × 220`
  fallback and the same `cfmakeraw`. Two of them in one crate would have been absurd, so they became
  `tddy_terminal_rpc::local_terminal`, used by both relays.
- **`cli.rs`'s `QuestionOption`** was a field-for-field copy of `tddy_core::backend::QuestionOption`.
  The moved `AskQuestionItem` re-exports the original; the bytes on the wire are identical either way.
- **A third spelling of "unset or blank".** `session_tool_client`'s private `non_empty_env` sat
  beside `mcp_primitives::env_non_empty` and `tddy_daemon_kernel::config::non_empty_env`. The first
  two are now `tddy_core::spawn_env::env_non_empty`. Collapsing them **settled a disagreement rather
  than picking a side**: one copy trimmed before testing for empty and the other did not, so a
  LiveKit variable exported as `" "` used to configure a transport whose URL was one space. It now
  reads as unset.

Two more are left standing and recorded:
[`MAX_MANIFEST_BYTES` twice and the third `non_empty_env`](../todo/2026-09-10-two-duplications-left-standing-by-the-tools-thinning.md).

Two things were deleted rather than moved for the same reason. **`restructure_cli::cli_vector()`**
re-serialized parsed clap arguments back into a `Vec<String>` to hand across a package boundary —
direct evidence that the CLI boundary was in the wrong place; with the dispatch inside
`tddy-code-restructuring` the parsed args are used as parsed. And **`tddy-tools`' `build.rs`**
generated a goal registry from a *sibling package's* directory, with `schema.rs` reaching in at
compile time through `include_dir!` and `include_str!`; once those modules live in
`tddy-workflow-recipes` the cross-package reach and the build script both go.

## Step zero: one module broke three import cycles

`action_tools`, `lsp_tools` and `session_agents/{seed,stream}` all imported back out of `server.rs`
over eight small shared items. Rust permits module cycles inside a crate, so nothing failed to
compile — but nothing could *leave* the crate either, because each of those edges becomes a
cross-crate cycle the moment one end moves. `mcp_primitives.rs` broke all three, and it gated every
other move in the node.

It traded three cycles for one, and the node then broke that too: `env_non_empty` went to
`tddy_core::spawn_env` (M7), `seed_subagents_or_report` and `subagents_from_env` to the roster's own
`seed.rs`, and `subagent_error_json` to `subagent_runtime` — which minted the envelope itself, for a
failed turn and for a conversation cancelled under its awaiters, so it could not stay behind.
`open_roster_agent_session` and `cancel_remote_conversation` stay in `mcp_primitives`: they are the
MCP surface's way of opening and closing a turn loop, and that edge is legal in both directions of
travel. Every remaining edge between the two crates points one way, and no file under
`packages/tddy-discovery/` contains the string `tddy_tools`.

The `tddy-testing-commons` ↔ `tddy-tools` prod/dev cycle — the only non-dev dependent
`tddy-tools` had anywhere — was resolved before any surface moved, by repointing
`tddy-testing-commons` at the destination crates.

## Five deviations from the plan, and they are the most useful thing here

Each is a case where the plan met the dependency graph and lost. The pattern across all five is one
rule the node discovered rather than started with: **the shape of an interface belongs to the crate
that speaks that interface** — MCP shapes stay where `rmcp` is, clap surfaces stay where arguments
are parsed, and a module goes where the types it manipulates already live.

**1 — Seam B stayed in `tddy-tools`.** The plan read the 14 `pr_*` and 2 `github_*` MCP tools as
~1,000 lines of PR-stack logic in the wrong crate. They are not: every one of the 16 bodies is a thin
adapter over a `tddy_workflow_recipes::pr_stack` function that already exists, wrapping the result in
a JSON envelope. What was available to move is ~415 lines of **MCP advertisement** and its `Pr*Input`
schemas — and moving those would put `rmcp` and `schemars` into `tddy-workflow-recipes`, which has
no MCP dependency at all, making every consumer of that crate (`tddy-coder` included) build `rmcp`.

**2 — Four of six `tddy-core` movers stayed, for three different reasons.** `session_hook` **cannot
move at all**: it imports `tddy_service::proto::connection`, and `tddy-service` depends on
`tddy-core`. That is a cycle, not a cost, and no dependency addition fixes it. `session_context`
moved verbatim once `anyhow` was added. `list_models` and `session_actions_cli` were **split, not
moved**: both parse clap arguments and both `println!` a JSON contract, and `tddy-core` is a library
the TUI links — CLAUDE.md forbids direct stdout in any path that runs under the TUI, so moving those
functions into it would put that hazard somewhere far more dangerous than a binary crate. The logic
went down and the surface stayed: `list_models` 172 → 52 with its catalogue assembly in
`tddy_core::backend::model_catalog`; `session_actions_cli` 136 → 66 with the session-directory
resolution in `tddy_core::session_actions::session_dir`.

**3 — `session_tool_client` went to a new crate, because `tddy-service` is a Cargo cycle.**
`dispatch_session_tool` selects between all four transports in one place, and its LiveKit arm calls
`tddy_livekit::client_connect::connect_client` — while `tddy-livekit` has `tddy-service` in
`[dependencies]`. Adding the edge yields `error: cyclic package dependency: package tddy-service
depends on itself`, and **`optional = true` behind a feature does not exempt it**. Three ways out
were rejected before the fourth was taken: splitting the selector would duplicate the transport
decision; injecting the LiveKit connector at runtime would reintroduce exactly the function-pointer
transport the same node was spending its budget deleting from `tddy-bsp`; and repointing the daemon
and darwin suites at `dispatch_via_sandbox_ipc` would delete their transport-detection coverage to
make a manifest assertion pass. So the client sits **above** both `tddy-service` and `tddy-livekit`,
in a twelfth destination the plan did not have. It costs the workspace nothing new — all six of its
dependencies were already `tddy-tools` dependencies — and it **discharges** the debt this changeset
had recorded against the original destination, that `tddy-service` depends on `tddy-tui` and would
have pulled the TUI into every in-jail binary that dispatches a tool call.

**4 — All five `session_agents` modules went to `tddy-discovery` together, because they cannot
split.** The plan gave four to `tddy-service` and left `registry.rs` for `tddy-discovery`.
`registry.rs` imports three `connection` protos, so its crate depends on `tddy-service`; `seed.rs`
and `stream.rs` import `registry::LiveAgentRoster`, so theirs is registry's crate or one above it —
and `tddy-service` is *below* it; `conversation.rs` implements
`tddy_discovery::subagent::SubagentSession`, which points the same way. Only a crate above both
`tddy-service` and `tddy-discovery`'s own types can hold all five, and that crate is `tddy-discovery`.

**5 — `relay.rs` stayed, because its planned destination was a name collision.** `tddy-discovery` is
the *codebase-exploration* agent; `relay.rs` is 119 non-blank lines that find or spawn a
**`tddy-daemon` relay process** — TCP probe, `daemon.json`, `--relay`. Moving it would also have put
`anyhow` into `tddy-discovery`, which has none, with no payoff, since nothing was blocked on it. It
turns out to have **no production caller anywhere in the workspace**: its only reader is its own
acceptance test. Finding it a real home, or retiring it, is
[its own entry](../todo/2026-09-10-tddy-tools-relay-rs-has-no-production-caller.md) rather than a
move made because a table said so.

Two smaller corrections belong to the same rule. `PtyRelayArgs` — a `#[derive(Args)]` struct with
twenty `#[arg]`s and their defaults — stayed, because `tddy-terminal-rpc` has no `clap`; the relay
itself moved, and node 6 compiles against the relay, not the flags. And seam C's MCP half stayed:
`build_dynamic_tool_list` returns `Vec<rmcp::model::Tool>` and `static_tool_names` names the MCP
server's own two always-registered tools, so moving them would put `rmcp` into `tddy-tool-engine`, a
crate every workspace-session host links.

## One deliberate behaviour change: 43 tools where they are served, 40 on the daemon path

The action tools — `request_action`, `list_actions`, `invoke_action` — are now advertised only when
the **host claims** it serves them, through `TDDY_SESSION_ACTION_TOOLS`. A session on a transport
whose host makes that claim advertises **43** tools; a daemon-hosted session advertises **40**.

This resolved a real defect rather than introducing one. The three were advertised whenever *any*
session-tool transport was reachable, and `tddy-daemon`'s handler dispatches into
`tddy_tool_engine::execute_tool_with_env`, whose `match` has no arm for any of them — so every call
came back `{"error":"unknown tool: ListActions","is_error":true}`. That is worse than absent: an
agent read the refusal as "the specialized agent is not registered" and reported it as such.

**The gate is a claim, not the transport, and that is the correction that mattered.** The blocking
todo asked for a gate on "a transport that serves them", which turns out not to be expressible:
`SessionToolTransport::SandboxIpc` is the transport for `tddy-sandbox-app`'s `AppToolHandler`, which
implements all three, **and** for `tddy-daemon`'s `DaemonToolHandler`, which implements none — and the
in-jail server cannot tell them apart from the socket. So the host says so, in the same shape as the
`TDDY_LSP_TOOLS` gate one line below it in the same router. The constant lives in
`tddy_core::session_actions`, the crate that owns actions and the only one both the advertiser
(`tddy-tools`) and the implementer (`tddy-sandbox-app`) already name.

`packages/tddy-tools/tests/mcp_tool_advertisement_audit.rs` pins **both sets by name** and pins the
difference *as a difference* — a tool added to both paths keeps it green, one added to only the
claiming path fails it. Asserting a single number would have meant choosing a transport and
pretending the other does not exist. The count rides on a `[&str; 43]` the compiler checks; the names
are what the assertions compare, because a count that still matched while a name changed is exactly
the failure the audit exists to catch.

The implementing alternative — three new daemon round-trips — was refused as a feature inside a
behaviour-preserving restructure.
[The blocking todo](../todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md)
is **closed** by the withdrawal.

## The env contract held, and it was audited by grep on purpose

Twenty-one real `TDDY_*` names, `TDDY_SOCKET` read at 43 sites, are how an in-jail agent reaches its
host. **None was renamed, none stopped being read**, and one was added
(`TDDY_SESSION_ACTION_TOOLS`). Eleven are now read from a different crate under an unchanged name and
an unchanged reading.

The plan's "25 distinct variables" was a raw-grep artefact: the same grep returns 23 tokens, two of
which are not variables (`TDDY_REMOTE_` is a prefix in a comment, `TDDY_TOOLS` the tail of
`USER_AGENT_TDDY_TOOLS`), and one of the remaining 21 — `TDDY_SUBAGENT` — was never read by
`tddy-tools` at all. The per-variable counts the same discovery recorded reproduce exactly, so the
method was sound and only the distinct-count was not. "17 of 34 dependencies drop" was likewise 14
dropped and 4 added against a base of 38.

**Why grep rather than a test.** A test asserting "`TDDY_SOCKET` is read in `tddy_tools::cli`" can
only re-read the source — it fails on a comment reflow and passes on a read that moved to a crate
nobody expected. What *is* testable is the observable behaviour these variables select, and all of it
already has tests: transport detection in `tddy-session-tool-client` plus the two sandbox suites, and
every advertisement gate over the real stdio wire in the advertisement audit, which sets six of these
variables and clears ten. The grep adds the one property none of those can see: a variable that
**nothing reads any more**. Checked set-against-set as well as name-by-name — 96 distinct tokens
across `packages/*/src` before, 100 after, and the difference in the "gone" direction is empty.

## Log targets moved with the code, except where they did not

Nineteen log-target strings moved so `target:` keeps naming where the code is:
`tddy_tools::session_context` → `tddy_core::session_context`, `tddy_tools::session_actions_cli` →
`tddy_core::session_actions::session_dir` (two of its four sites; the other two stayed with the
surface), `tddy_tools::session_tool_client` → `tddy_session_tool_client`, and
`tddy_tools::session_agents` → `tddy_discovery::{roster, subagent_runtime}`. Errors are unaffected —
they pass the default `warn` filter whatever the target — but an operator watching the roster stream
with `RUST_LOG=tddy_tools=debug` now wants `RUST_LOG=tddy_discovery=debug`.

⚠ **Twenty-two sites did not move, and the stack now ships two contradictory rules.** Schema
validation and the GitHub PR client still emit `tddy_tools::schema`, `tddy_tools::schema_manifest`
and `tddy_tools::github_pr` from inside `tddy-workflow-recipes` — `schema.rs` 6, `schema_manifest.rs`
3, `github_pr.rs` 13. Node 4's recorded policy is the **opposite** one: keep the target, because
renaming silently breaks an operator's `RUST_LOG` filter and the failure mode is *missing logs*.
Both positions are defensible; shipping both is not, and deciding which is
[its own entry](../todo/2026-09-10-schema-validation-still-logs-under-the-tddy-tools-target-after-moving-crates.md).

## One rule, argued two ways — recorded rather than repaired

This node justifies keeping `list_models`, `session_actions_cli`, `PtyRelayArgs` and seam B in
`tddy-tools` on two rules: *stdout must not go where the TUI links*, and *the clap surface stays
where arguments are parsed*. M3 applied neither to `build_cli`. `tddy_bsp::build_cli` now holds six
direct-stdout sites and a hard `exit(1)`, and **`tddy-coder` links `tddy-bsp`** — so the exact hazard
the `list_models` decision refused to create is present one crate over. `clap` entered three library
crates in the same wave (`tddy-bsp`, `tddy-code-analysis`, `tddy-code-restructuring`) where the later
milestones kept it out of `tddy-terminal-rpc` and `tddy-workflow-recipes` on principle.

Neither is a live TUI corruption today — nothing in the TUI's own paths calls `build_cli`'s dispatch,
and `clap` in a library is a build cost rather than a hazard — but the earlier milestones took
decisions the later ones would not have. Splitting `build_cli` the way `list_models` was split is a
follow-up.

The same asymmetry appears in a manifest. `tddy-workflow-recipes` gained three **production**
dependencies at M4 (`include_dir`, `jsonschema`, `tempfile`) — and seam B was then refused
*specifically* to avoid adding dependencies to that crate. Both calls may be right on their merits:
`jsonschema` is a smaller build than `rmcp`, and `schema.rs` cannot be split from the data it
validates the way seam B's adapters split from `pr_stack`. Reversing M4 would put the cross-package
`include_dir!` reach and `tddy-tools`' `build.rs` back, so it stands — but ten crates depend on
`tddy-workflow-recipes` against `tddy-tools`' four, so in-jail binaries that reach the recipes crate
now build `jsonschema`.

## The one genuine behaviour drift, found in review and fixed

M3 deleted `cli_vector()` and handed clap's parsed `Vec<String>` straight to the restructure runner.
But the pre-move path did not hand it over raw: `runner::comma_separated` ran it through
`.split(',').map(str::trim).filter(|i| !i.is_empty())`, and clap's `value_delimiter = ','` splits on
the comma and stops there.

| `restructure anchors --items …` | Before | After M3 | Now |
|---|---|---|---|
| `"One,Two"` | `["One", "Two"]` | `["One", "Two"]` | `["One", "Two"]` |
| `"One, Two"` | `["One", "Two"]` | `["One", " Two"]` ❌ | `["One", "Two"]` |
| `"A,,B"` | `["A", "B"]` | `["A", "", "B"]` ❌ | `["A", "B"]` |

**The failure mode is what makes it worth recording: no error.** An items list written the way a
human writes one resolved an item literally named `" Two"`, the anchor came back not covering it, and
nothing said why. Instructive too is why the verification missed it — the test guarding that call
site used `"One,Two,Three"`, the one input on which the two normalisations agree. Fixed by
`normalised_items` and pinned by two new tests, both observed failing against the unfixed call site
first.

## Three crates gained a transitive `tddy-tui` dependency

`tddy-terminal-rpc`, `tddy-session-tool-client` and `tddy-discovery` all needed `tddy-service` — the
relay encodes `connection.ConnectionService` and `auth.AuthService` frames, the client sends nothing
that is not a `tddy-service` proto, and the roster speaks `connection.SessionAgentService`. And
`tddy-service` depends on `tddy-tui`.

It costs least in `tddy-terminal-rpc`, whose three reverse-dependencies all depend on `tddy-service`
directly already, and most in `tddy-discovery`, which has eight — the in-jail ones now build the TUI
to follow a roster. `tddy-session-tool-client` is the one that *avoided* it, by existing at all: the
client above `tddy-service` links no TUI, which is why the original destination was a debt even
before it turned out to be a cycle. There was no move that avoided the other two, so all three are
[recorded](../todo/2026-09-10-three-crates-gained-a-transitive-tddy-tui-dependency.md) and all three
are fixed by the same change — splitting the protos out of `tddy-service`.

## Files over budget

One of the five original offenders landed under budget and four did not, and two new files land over
it. Measured in non-blank lines.

| File | Before | After | |
|---|---:|---:|---|
| `tddy-tools/server.rs` | 3,842 | 3,121 | seams A, B and E, kept on purpose |
| `tddy-session-tool-client/lib.rs` | 991 | 1,005 | moved verbatim; splitting the four-transport selector is the one thing the move refused |
| `tddy-tools/cli.rs` | 927 | 833 | twenty-odd clap dispatch arms; splitting them is a different change |
| `tddy-tools/pty_relay.rs` | 843 | 193 **+ 766** in `tddy-terminal-rpc` | the only original offender under budget here; the four dispatch modes are not separable |
| `tddy-discovery/roster/registry.rs` | 685 | 742 | +57, absorbing the two `seed` helpers that made the pair mutual |
| `tddy-discovery/subagent_runtime.rs` | — | 549 | seam D's runtime, 172 of it tests; the tool bodies that stayed drive it |
| `tddy-tool-engine/lib.rs` | 624 | 703 | already over before this node; +79 from the catalog and the denial predicate |

Nothing else in the fifteen touched packages crossed 500 because of this node.

## Baseline

| Gate | Result |
|---|---|
| `cargo test` per package, `--test-threads=4`, sequentially | **12 of 13 green** — `tddy-tools` 320, `tddy-core` 589, `tddy-workflow-recipes` 559 (+1 ⚠), `tddy-code-restructuring` 296, `tddy-service` 111, `tddy-discovery` 109, `tddy-code-analysis` 34, `tddy-terminal-rpc` 24, `tddy-testing-commons` 24, `tddy-bsp` 12, `tddy-tool-engine` 11, `tddy-lsp-executor` 6, `tddy-session-tool-client` 5 |
| `cargo clippy --all-targets -- -D warnings` | ✅ exit 0 across all **15** packages this node touched |
| `cargo fmt --all -- --check` | ✅ |
| advertised MCP tools over the real `--mcp` stdio wire | 43 with the host's action claim, 40 without, pinned by name |

⚠ **One failing test, not this node's.** `tddy-workflow-recipes`'
`pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`
expects `/tmp/nix-shell.…` and gets `/private/tmp/nix-shell.…` — macOS canonicalises `/tmp`, Linux
does not, so it fails locally and passes on CI. **Run that crate with `--no-fail-fast`**: `cargo
test` stops after the first failing *target*, so a plain run reports 363 of the 559 tests and hides
four whole targets.

⚠ **One hang that will wedge an unattended run.** `tddy-terminal-rpc`'s
`local_pty_relay::runs_a_fast_exiting_command_to_completion` never returns when the test process'
stdin is an open pipe that never delivers EOF — which is what a detached shell gives it. It passes in
~0.1s with stdin on `/dev/null`. The relay code is byte-identical to its pre-node form, so the hang
predates the move;
[characterised here](../todo/2026-09-10-local-pty-relay-never-returns-when-its-stdin-is-an-open-pipe.md).

**Six of the node's sixteen red tests were deleted rather than made to pass**, because they asserted
designs the code does not have — a fabricated two-state `RosterCurrency` where the real one has four
and is private, an `addressable()` and a `CatalogVisibility` enum that do not exist, a
`RelayMode`/`RelayError` pair that never existed, and an advertisement filtered by the roster, which
it must not be: `--allowedTools` is fixed when `claude` spawns, so a takeover can only be enforced at
dispatch. What replaced them is the moved code and the integration suites that already drive it
through the real `--mcp` wire.

## Where the draft surface lost to the real one

Two of the draft PR's stub surfaces were invented outright, and both are worth stating because
successors compile against them. The tool client's declared `SessionToolTransport::from_env()`,
`SessionToolError` enum and typed `Result` do not exist — detection is a free function, dispatch
returns a `String`, and **every failure is a `{"error": …, "is_error": true}` JSON string, because
it is the model's answer**. The roster's declared `RosterCurrency`, `currency()`, `addressable()` and
`CatalogVisibility` enum do not exist either; `RosterCurrency` has four states and **stays private**,
because publishing an internal state machine to match a fabrication would make it contract — and the
stub's own test shows what that invites, asserting `Current` for a roster the real design calls
`Seeded`, which is exactly the conflation the four states exist to prevent.

## Open items

Recorded in `docs/dev/todo/`:
[the permission engine's NDJSON socket protocol survives](../todo/2026-09-10-the-permission-engines-ndjson-unix-socket-protocol-survives.md),
[the `execute-tool-stdio-fixture` bin forces three dev-deps into `[dependencies]`](../todo/2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md),
[`relay.rs` has no production caller](../todo/2026-09-10-tddy-tools-relay-rs-has-no-production-caller.md),
[`tddy-livekit` depends on `tddy-service` for two call sites](../todo/2026-09-10-tddy-livekit-depends-on-tddy-service-for-two-call-sites.md),
[three crates gained a transitive `tddy-tui` dependency](../todo/2026-09-10-three-crates-gained-a-transitive-tddy-tui-dependency.md),
[two duplications left standing](../todo/2026-09-10-two-duplications-left-standing-by-the-tools-thinning.md),
[schema validation still logs under a `tddy_tools::` target](../todo/2026-09-10-schema-validation-still-logs-under-the-tddy-tools-target-after-moving-crates.md),
[`local_pty_relay` never returns when its stdin is an open pipe](../todo/2026-09-10-local-pty-relay-never-returns-when-its-stdin-is-an-open-pipe.md).

Closed: [the action tools are advertised where nothing implements them](../todo/2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md)
— withdrawn behind the host's claim, with the advertisement audit as its regression test.
