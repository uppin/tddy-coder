# Initial Discovery: Subagent Turn Control and Honest Tool Failure

**Changeset**: [2026-09-26-subagent-turn-control-and-honest-tool-failure.md](./2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
**Date**: 2026-09-26
**Passes**: 3

## Combined Conclusions

### What started this

Live incident, session `01a0dd54-71ad-7422-865e-d39a8db28352` (Claude CLI) paired with workspace
session `01a0dd54-71ad-7422-865e-d3a0b994d57d`, on 2026-09-26. Evidence read from
`~/.tddy/sessions/*/tool-calls.jsonl`, `~/.tddy/logs/daemon`, and the still-live process tree.

The chain, verified end to end:

1. **13:49:22** — a `Shell` tool call ran `f=$(ls … 2>/dev/null | head -3); grep -n -A30 "interface ITheme" $f | head -45`.
   `ghostty-web` is not installed, so `$f` was empty and `grep` fell back to **reading stdin**.
2. **13:49:52** — the tool returned `Shell: timed out after 30000ms`. It killed nothing.
   `tokio::time::timeout` drops the future; `kill_on_drop` defaults to `false`.
3. The orphan inherited the runner's **stdin**, which for `tddy-sandbox-runner --stdio` is the
   tool-IPC request pipe. Confirmed by `lsof`: `grep` pid 45895 fd 0 and runner pid 44928 fd 0 are
   the same pipe (`0x48c5b92d7e4d7d2 -> 0xa415b74f31505e83`). The orphan is a rival reader on the
   daemon→jail request channel.
4. **13:50:14** — the next tool call blocks. `InJailChannel` allows one outstanding call under a
   mutex (`workspace_tool_sandbox.rs:389-396`), so 13:52:17 queues behind it.
5. **14:00:14** — `IN_JAIL_TOOL_TIMEOUT` (600s) fires: `it did not answer within 600s`. The channel
   is set to `None` and, by design, **stays** `None` (`workspace_tool_sandbox.rs:404`).
6. **14:02:45 onwards** — FastContext is consulted. **54 tool calls, every one refused** with
   `the tool call could not be run in its jail (its channel is closed)`. It read nothing.
7. It burned all 10 turns (2 calls per turn), then `run_synthesis_turn` forced one tool-less turn
   whose instruction is *"Summarize your findings now from what you have already read, **citing the
   specific file:line locations you found**."* With zero successful reads, the model invented
   line 108, `PAGE_SCROLLBACK = 10000`, and a working directory `/taddy`.
8. Prompted again at 14:05:48 on the **same** conversation (33 messages, continuing from 32) it
   produced byte-identical output — 1470 chars both times. `temperature: 0.0`
   (`subagent.rs:615`) makes that deterministic, not a cache.

### The shape of the change

Five defects and two new capabilities, all on one causal path: **a specialized agent's tool
failures are invisible, and its turn budget is not the caller's to set.**

| # | Defect / capability | Root cause site |
|---|---|---|
| D1 | A jailed shell inherits the runner's IPC stdin | `tddy-tool-engine/src/lib.rs:523`, `:128`, `shell.rs:88` |
| D2 | The Shell timeout abandons rather than kills | same three sites — no `kill_on_drop`, no group kill |
| D3 | A broken jail channel is permanently dead with no relaunch | `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs:404`, `:421-445`; retry belongs at `local_exec_tools.rs:119` |
| D4 | A prompt whose every tool call failed still asks the model for a cited summary | `tddy-discovery/src/subagent.rs:1056-1095` |
| D5 | `Read`'s advertised `offset`/`limit` are ignored by the engine | `tddy-tool-engine/src/catalog.rs:21` vs `lib.rs:302-320` |
| C1 | The MCP caller cannot set a turn budget | `max_turns` comes only from the def (`subagent.rs:1200`) |
| C2 | A conversation cannot be resumed or rewound | no surface at all |

### The one sub-problem both halves share

Neither half of this change can be written until failures can be told apart by **kind**.

- **Jail side.** `WorkspaceSandbox::execute_tool` reports a dead channel as
  `ExecuteToolResponse { is_error: true, .. }` — shaped identically to a failing `Shell`. The
  trait doc says so on purpose (`workspace_tool_sandbox.rs:41-46`). A retry keyed on `is_error`
  would relaunch the jail every time a command exits non-zero.
- **Subagent side.** `dispatch_tool_call` returns a bare `String` and shapes failure as
  `format!("{{\"error\": \"{e}\"}}")` (`subagent.rs:588`) — no `is_error`, unescaped.

So the first milestone is a **typed transport-failure signal**, on both surfaces. D3's retry and
D4's hard error are each a few lines once it exists, and unbuildable before.

### Where the relaunch actually goes — correcting the option as it was framed

`relaunch_sandboxed_runner` relaunches the **claude-cli** jail (`sandbox_manager`) and never
touches `workspace_sandboxes`. It is the wrong function for the jail that broke. The workspace
jail's relaunch primitive already exists and is smaller: `JailedWorkspaceSandboxProvisioner` is a
unit struct whose `provision(&self, spec)` is stateless, and
`provision_workspace_tool_sandbox` / `reprovision_colocated_checkout_jail` already wrap it as an
idempotent get-or-provision. Auto-relaunch is therefore **cheaper** than the option described,
not bigger.

The retry has exactly one viable home: `LocalExecTools::run_exec_tool_locally`
(`local_exec_tools.rs:88-156`, jail dispatch at `:119`). It is the only layer that holds the
registry *and* sits under all three dispatch entries — including `svc_start_hosted_agent_clone.rs:281`,
which is the FastContext path. It needs one new constructor field
(`Arc<dyn WorkspaceSandboxProvisioner>`), and `LocalExecTools::new` has a single call site.

### The decisive architectural fact

**In this deployment every subagent conversation is _remote_.** `subagent_new_session`
(`tddy-tools/src/server.rs:1760`) runs the loop locally only when this process holds the def;
otherwise it opens a `RemoteAgentSession` over `OpenAgentConversation`. The jail's `tddy-tools`
holds no defs — the daemon log records `spawn seed carries 0 specialized agent def(s)` — and the
incident's `SpecializedSubagentSession` lines appear in the **daemon's** log, not the jail's.

Consequences for C1 and C2, and they are not optional:

- `max_turns`, resume, rewind and message ids **must cross `session_agents.proto`**, not just the
  Rust trait. `PromptAgentConversationRequest` (proto:141-147) has no turn budget;
  `AgentConversationChunk` (proto:80-89) carries only `content_chunk`, `stop_reason`, `last`.
- `RemoteAgentSession::tail` returns `Vec::new()` with a standing TODO
  (`roster/conversation.rs:291-297`), so there is no remote history surface to build ids on.
- `SubagentSession::prompt(&mut self, text: &str)` is a **trait** method with two implementors.
  A per-call budget and a resume change the trait, and therefore both.

### Tension to resolve deliberately

`server.rs:1755-1759` states the intent verbatim: *"A roster entry carries no endpoint, credential
or **turn budget** — deliberately, so editing a def cannot change what a running session may
call."* A caller-supplied `maxTurns` moves budget control to the caller. That is not the same thing
as editing a def, but it needs an explicit bound or a main agent can spend unbounded local-model
time. Recorded as a decision in the changeset: the def's `max_turns` is the **default**, a per-call
value may exceed it, and both are clamped to an absolute ceiling.

### Second-order findings, all verified

- **`DEFAULT_READ_LINE_CAP` is not enforced on the managed path.** `window_content`
  (`subagent.rs:415`) is applied only under `CodebaseAccess::Local`. The `Managed` branch
  (`subagent.rs:195-206`) forwards `offset`/`limit` to the daemon and returns whatever comes back —
  and the daemon's `tool_read` ignores both. So a jailed subagent's context fills with whole files.
  The contract was tested on the sending side only:
  `read_window_red.rs:120 managed_read_window_forwards_offset_and_limit_to_the_read_tool` passes;
  nothing tests the receiver. `read_output_cap_red.rs` covers `Local` exclusively.
- **A tool result carries no error flag.** `dispatch_tool_call` returns a bare `String` and shapes
  failure as `format!("{{\"error\": \"{e}\"}}")` (`subagent.rs:588`) — no `is_error`, and the error
  text is interpolated into JSON **unescaped**, so a message containing `"` or a newline yields
  invalid JSON. There is therefore no seam today on which "every tool call failed" could be
  detected. Building one is a prerequisite for D4.
- **`TurnEnd::took_a_turn` rests on a false premise.** Its comment says a failure *"added nothing
  to the history"* (`subagent_runtime.rs:495-497`). Messages are pushed as the loop runs, so a
  failed prompt has already grown the history. Under the chosen hard-error behaviour for D4 this
  stops being cosmetic: the conversation must remain resumable after an error.
- **Usage is parsed correctly; the zero is downstream.** Ollama returns
  `{"prompt_tokens":9,"completion_tokens":5}` (verified live against `localhost:11434`) and
  `deserialize_usage` (`openai.rs:340-356`) maps exactly those. `AgentConversationChunk` carries no
  usage at all, which is why a *remote* conversation reports zero — `RemoteAgentSession::cumulative_usage`
  returns `TokenUsage::default()` with a comment saying so (`roster/conversation.rs:275-278`).
- **The FastContext assistant row has an empty `system_prompt`** (`models.db`, `assistant` table).
  No identity, no working directory, no instruction against inventing content. Data, not code.
- **`max_turns` is not a column in `models.db`.** `assistant_def.rs:53` hardwires
  `DEFAULT_MAX_TURNS = 10` for every registry assistant. A per-call override makes a column
  unnecessary for this change.

### Correction to the incident write-up

The all-caps tool names FastContext reported (`READ`, `GREP`, `WRITE`, `EXEC`, `LIST`) are **not
all fabricated**. `dispatch_tool_call` (`subagent.rs:536-583`) dispatches on exactly
`READ`/`GLOB`/`GREP`/`WRITE`/`STR_REPLACE`/`DELETE`/`SHELL`/`AWAIT`/`READ_LINTS`/`SEMANTIC_SEARCH`.
Three of the five it named are real bound tool names; only `EXEC` and `LIST` were invented.

### Conflicts with other WIP changesets

**None live.** Two files in `docs/dev/1-WIP/` look like conflicts and are not:

- `2026-09-20-specialized-agent-context-handoff.md` — shipped in **#521, merged 2026-09-20**.
  Its code is in master (`StopReason::ContextExhausted`, `context_exhausted_outcome`).
- `2026-09-20-subagent-turn-queue-visibility.md` — same PR, also merged. `queuePosition`/`queueSize`
  are in master at `tddy-tools/src/server.rs:1692-1704`.

Both were never wrapped. They are stale working documents sitting in the active directory. Flagged
in the changeset's Prerequisites; closing them is a wrap-hygiene task, not this change's work.

### Testing constraints discovered

`packages/tddy-daemon/tests/in_jail_conversation_acceptance.rs` — the natural home for a real-jail
proof — **runs on no machine**. It carries `#![cfg(target_os = "macos")]` as an inner attribute so
Linux CI compiles an empty binary, and on macOS it fails in setup about three runs in four due to a
stdio-bridge attach race
([`2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)).
The test plan therefore cannot lean on a real jail. `CodebaseAccess::managed(...)` plus a
`wiremock` provider is the injection point that reaches the same logic without one — the pattern
`subagent_context_exhaustion_red.rs` already uses.

Pass 3 refined this. The suite is **not** under `tddy-daemon-sandbox/tests/`; it is
`packages/tddy-daemon-rpc/tests/in_jail_conversation_acceptance.rs:28`, and it is not `#[ignore]`d
— it is macOS-gated while every CI job is `runs-on: ubuntu-*`, so it compiles to an empty binary.
What *does* run on CI and can gate this change: `tddy-sandbox-runner/tests/in_jail_tool_dispatch.rs`
and `in_jail_tool_dispatch_logging.rs` (both ungated, and the latter's doc asserts the very
invariant this change alters), `host_relay_dispatch.rs`,
`tddy-daemon-sandbox/tests/workspace_tool_sandbox_plan_unit.rs` (the only always-running coverage
of the module), and `tool_catalog_sync.rs` (which a `Read` schema change touches).

## Exploration 1: Live incident forensics and root-cause reading — 2026-09-26

**Agent**: parent Grep/Glob/Read + Bash
**Scope**: `~/.tddy` session records and daemon logs; `tddy-tool-engine`, `tddy-daemon-sandbox`,
`tddy-sandbox-runner`, `tddy-discovery` sources; the two deferred-work records.

### Sequence

1. `ls ~/.tddy` and `find ~/.tddy -name '*01a0dd54*'` — locate the reported session. Found a
   **pair**: `…d39a8db28352` (`session_type: claude-cli`) and `…d3a0b994d57d`
   (`session_type: workspace`, `sandbox: true`), cross-linked by `codebase_session_id` /
   `agent_session_id`.
2. `cat .session.yaml` on both — established the roster: `FastContext` (`fastcontext-tools-32k:latest`,
   `replaces: [Grep, Glob, SemanticSearch]`) and `Gemma Local Coder` (`gemma4:e4b-mlx`,
   `replaces: [Write, StrReplace, Delete]`).
3. `cat tddy-tools.mcp.log` — server initialized, **no tool calls logged**. The MCP server was not
   the failing hop.
4. Parsed `…d3a0b994d57d/tool-calls.jsonl` (12 records) with a Python one-liner printing
   `created_unix_ms`, `tool_name`, `is_error`, `job_running`.
5. Cross-referenced against `~/.tddy/logs/daemon` `ExecuteTool` dispatch lines to pair
   dispatch→completion and find the unpaired ones.
6. `ps -eo pid,ppid,etime,stat,command | awk '$2==44928'` — found the surviving `sh -c`.
7. `lsof -p <pid> -a -d 0,1,2` on runner, `sh`, `grep`, `head` — the decisive evidence.
8. Read `tddy-tool-engine/src/lib.rs`, `shell.rs`, `catalog.rs`;
   `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`; `tddy-sandbox-runner/src/host_relay.rs`;
   `tddy-discovery/src/subagent.rs`, `subagent_runtime.rs`, `tools.rs`, `roster/conversation.rs`,
   `openai.rs`.
9. `sqlite3 ~/.tddy/models.db` — assistant and provider rows.
10. `curl localhost:11434/v1/chat/completions` — confirmed the provider reports usage.
11. Scanned both deferred-work records (commands in Exploration 1 → *Grep / glob* below).

### Grep / glob

| Pattern | Scope | Notable hits |
|---|---|---|
| `"timed out after"` | `packages --include=*.rs` | `tddy-tool-engine/src/shell.rs:441`, `lib.rs:545` |
| `"channel is closed"` | `packages --include=*.rs` | `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs:425` — the only site |
| `IN_JAIL_TOOL_TIMEOUT` | repo | declared `tddy-sandbox-runner/src/host_relay.rs:276` = `600s`; consumed `workspace_tool_sandbox.rs:488` |
| `DEFAULT_MAX_TURNS` | `packages --include=*.rs` | `tddy-model-registry/src/assistant_def.rs:13`, used `:53` |
| `offset` | `tddy-tool-engine`, `tddy-discovery` | advertised `catalog.rs:21`; discarded `tddy-discovery/src/tools.rs:56-57`; never read in `tddy-tool-engine/src/lib.rs` |
| `Claimed by:` | `packages/*/docs/code-issues/` | 2 hits, both unrelated packages |
| `subagent\|jail\|workspace_tool\|max_turns\|stdin` | `docs/dev/todo/` | 48 files; 6 read in full |
| `grep -c "01a0dd54"` | `~/.tddy/logs/daemon` | 14 lines — session start only, no tool traffic |
| `"could not be run in its jail"` | `~/.tddy/logs/daemon` | **54 occurrences, all naming `…d3a0b994d57d`** |

### Inspected files and excerpts

**`packages/tddy-tool-engine/src/lib.rs:523-545`** — the blocking Shell path:

```rust
let fut = tokio::process::Command::new("sh")
    .arg("-c").arg(&command).current_dir(root)
    .envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    .output();
let outcome = match tokio::time::timeout(timeout, fut).await {
    …
    Err(_) => ToolOutcome::err(format!("Shell: timed out after {}ms", block_until_ms)),
};
```

No `.stdin(…)`, no `.kill_on_drop(true)`. Verified against tokio 1.53.1
(`~/.cargo/registry/src/…/tokio-1.53.1/src/process/mod.rs:1065-1072`) that `Command::output()`
sets **only** stdout and stderr — unlike `std::process::Command::output()`, it leaves stdin
inherited. Two sibling sites share the defect: `lib.rs:128-135` (`ShellTaskBody`, background jobs)
and `shell.rs:87-95` (`LocalShell::run`).

**`packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs:398-446`** — the jail, and why it
stays dead:

```rust
struct JailedWorkspaceSandbox {
    session_id: String,
    pid: u32,
    handle: StdMutex<Option<tddy_sandbox::SandboxHandle>>,
    /// `None` once the channel is gone: a jail whose channel broke stays broken, because the only
    /// alternative to answering from inside it is answering from the host it was built to avoid.
    channel: Mutex<Option<InJailChannel>>,
}
```

and on any exchange error:

```rust
// A channel that lost its answer cannot be reused: the next response would be
// matched to the wrong request.
*guard = None;
```

**`packages/tddy-discovery/src/subagent.rs:1056-1095`** — `run_synthesis_turn`, the fabrication
site. Its own doc comment explains why the *instruction* is never retained but says the summary
**is** kept: *"it answers the user prompt that is still in the history."* The prompt spliced in:

```rust
request_messages.push(ChatMessage::user(
    "You have reached your search budget and may not call any more tools. \
     Summarize your findings now from what you have already read, citing the specific \
     file:line locations you found."
        .to_string(),
));
```

There is no check that any tool call succeeded.

**`packages/tddy-discovery/src/subagent.rs:1100-1149`** — `prompt`. Budget loop
`for _turn in 0..self.max_turns`, then unconditional synthesis on exhaustion.

**`packages/tddy-discovery/src/subagent.rs:532-590`** — `dispatch_tool_call`. All-caps tool names;
`Err(e) => format!("{{\"error\": \"{e}\"}}")` with no escaping and no `is_error`.

**`packages/tddy-discovery/src/subagent.rs:186-206`** — `read_window`. `Local` applies
`window_content`; `Managed` forwards `offset`/`limit` and returns the dispatch result unwindowed.

**`packages/tddy-tool-engine/src/lib.rs:302-320`** — `tool_read`. Reads `path` only;
`std::fs::read_to_string` then `{"content": content}`. `offset` and `limit` never mentioned.

**`packages/tddy-discovery/src/roster/conversation.rs:261-297`** — `RemoteAgentSession`.
`cumulative_usage` → `TokenUsage::default()`, `context_tokens` → `0`, `tail` → `Vec::new()`, each
with a TODO naming the missing RPC.

**`packages/tddy-discovery/src/subagent_runtime.rs:432-522`** — `DeferredTurn` (carries a
non-optional `prompt_text: String`), `run_turn`, and `TurnEnd`.

**`~/.tddy/logs/daemon`**, the two decisive lines:

```
14:00:14.383 [WARN] … session 01a0dd54-…-d3a0b994d57d: the tool call could not be run in its jail
             (it did not answer within 600s); refusing to run it on the host worktree instead
14:00:14.385 [WARN] … (its channel is closed); refusing to run it on the host worktree instead
```

### Findings

1. D1–D5 as tabulated in Combined Conclusions, each with a named site.
2. The 600s deadline exists and works; it is simply far longer than an interactive turn, and it
   neither kills the jail nor reaps the orphan that caused the stall.
3. Every one of the 54 refusals names the *same* session, which is what makes FastContext's
   failure a downstream consequence rather than an independent bug.
4. Two `docs/dev/1-WIP/` changesets are merged-but-unwrapped (#521).
5. The real-jail acceptance suite is unusable as a gate.

## Exploration 2: The specialized-subagent MCP and RPC surface — 2026-09-26

**Agent**: Explore subagent
**Scope**: `tddy-tools` MCP tool declarations and handlers; `session_agents.proto` and its Rust
service; `tddy-discovery` session types; `tddy-model-registry` assistant projection; existing tests.

### Findings

**All six `subagent_*` tools live in one file**, `packages/tddy-tools/src/server.rs`.
`tddy-tool-engine/src/catalog.rs` holds none of them — it is the exec-tool catalog only.

| Tool | Declared | Handler | Schema |
|---|---|---|---|
| `subagent_new_session` | `:2358-2368` | `:1739-1799` | built by `subagent_new_session_schema` `:2247-2269` (the only roster-dependent schema) |
| `subagent_prompt` | `:2370-2390` | `:1812-1906` | `subagent_prompt_schema` `:2304-2329` |
| `subagent_await` | `:2392-2407` | `:1913-1934` | `subagent_await_schema` `:2332-2349` |
| `subagent_cancel` | `:2409-2422` | `:1937-1980` | inline |
| `subagent_list` | `:2424-2446` | `:1984-1987` | `{"type":"object","properties":{}}` |
| `subagent_status` | `:2448-2484` | `:2132+` | inline, with `waitFor` enum |

`subagent_prompt`'s full input schema, verbatim:

```json
{"type":"object","required":["sessionId","prompt"],"properties":{"sessionId":{"type":"string"},"prompt":{"type":"array","items":{"type":"object","required":["type","text"],"properties":{"type":{"type":"string"},"text":{"type":"string"}}}},"graceMs":{"type":"integer","minimum":0,"description":"How long to block for the turn before returning a responseId to collect it with. Defaults to 25000; 0 defers immediately."}}}
```

Router merged unconditionally at `server.rs:339`; **advertisement** is gated on the live roster in
`advertised_tools()` `:400-425`. Each route is wrapped by `subagent_route`
(`mcp_primitives.rs:95-125`). `SUBAGENT_PROMPT_GRACE = 25s` at `server.rs:1679`.

**The sandboxed-Claude allowlist is a separate list that must be kept in step** —
`packages/tddy-sandbox-recipes/src/claude_cli.rs:169-177`:

```rust
const SUBAGENT_TOOLS: &[&str] = &[
    "mcp__tddy-tools__subagent_new_session",
    "mcp__tddy-tools__subagent_prompt",
    "mcp__tddy-tools__subagent_await",
    "mcp__tddy-tools__subagent_cancel",
];
```

`subagent_list` and `subagent_status` are deliberately absent. A new `subagent_resume` that is not
added here is advertised but uncallable from a sandboxed Claude.

**`session_agents.proto`** (`packages/tddy-service/proto/session_agents.proto`), nine methods.
The two that matter:

```proto
message PromptAgentConversationRequest {
  string session_token = 1;
  string session_id = 2;
  string daemon_instance_id = 3;
  string conversation_id = 4;
  string prompt = 5;
}

message AgentConversationChunk {
  string content_chunk = 1;
  string stop_reason = 2;
  bool last = 3;
}
```

No turn budget in, no usage or message ids out. `IN_JAIL_RELAYABLE`
(`packages/tddy-service/src/session_agents.rs:67-73`) allowlists exactly five operations; a new
resume RPC must be added there or the jail cannot call it.

**`SubagentSession` trait** (`subagent.rs:91-121`) — five methods, two implementors:

```rust
#[async_trait]
pub trait SubagentSession: Send {
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError>;
    fn model(&self) -> &str;
    fn cumulative_usage(&self) -> TokenUsage;
    fn context_tokens(&self) -> u64;
    fn tail(&self, max_messages: usize) -> Vec<String>;
}
```

`StopReason` (`:42-56`) is `EndTurn | MaxTurnRequests | Cancelled | ContextExhausted`, wire
spellings parsed in `roster/conversation.rs:303-317` where an unknown spelling is a hard error —
so **any new stop reason is a two-sided change**. (The chosen hard-error behaviour for D4 adds
none.)

**`max_turns` flow**: def field `agent_def.rs:132-133` with `default_max_turns() -> 10` at `:96-98`
→ `SubagentRegistry::create` passes `def.max_turns` at `subagent.rs:1200` → consumed at
`subagent.rs:1104`. Registry assistants bypass the def file entirely:
`assistant_to_agent_def` hardwires `max_turns: DEFAULT_MAX_TURNS` at
`tddy-model-registry/src/assistant_def.rs:53`. The `assistant` table DDL
(`tddy-model-registry/src/store.rs:1057-1072`) has **no `max_turns` column**, and `AssistantEntry`
(`packages/tddy-service/proto/models.proto:115-132`) has no such field.

**Tests that constrain the change**:

| File | Why it matters here |
|---|---|
| `tddy-tools/tests/mcp_tool_advertisement_audit.rs` | pins the entire advertised surface **by name** — 43 tools with the action surface, 40 without. Adding `subagent_resume` moves both counts. |
| `tddy-tools/tests/subagent_async_response_acceptance.rs` | the `responseId` / grace / await contract; `:733` asserts the prompt **description** documents both shapes |
| `tddy-tools/tests/subagent_mcp_acceptance.rs` | ACP-shaped wire contract over real `--mcp` stdio; `:251` prompt ping-pong retains history |
| `tddy-tools/tests/subagent_tool_advertisement_acceptance.rs` | advertisement tracks the live roster |
| `tddy-sandbox-recipes/src/claude_cli.rs:478` | `claude_allowlist_offers_subagent_await_exactly_where_it_offers_subagent_prompt` |
| `tddy-discovery/tests/subagent_context_exhaustion_red.rs` | the closest analogue: `wiremock` provider + `SubagentRegistry::from_defs(…).create(…)` |
| `tddy-discovery/tests/read_window_red.rs:120` | asserts the subagent **forwards** `offset`/`limit`; nothing asserts the engine honours them |

## Exploration 3: The workspace jail lifecycle and where a relaunch can live — 2026-09-26

**Agent**: Explore subagent
**Scope**: `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs` and `sandbox_session.rs`; the
`relaunch_sandboxed_runner` path; the full `ExecuteTool` → jail trace; jail test inventory.

### Findings

#### `relaunch_sandboxed_runner` is the wrong primitive for this jail

`svc_relaunch_sandboxed_runner.rs:57` relaunches the **claude-cli** jail, registered in
`self.sandbox_manager` (a `SandboxSessionManager`). It never touches `self.workspace_sandboxes`.
Its single call site is `svc_split_context_from_codebase_host.rs:405`, from
`resume_sandboxed_claude_cli_session` (`:373-432`), which likewise only removes from
`sandbox_manager`.

The jail that broke in the incident is the **workspace tool jail** —
`JailedWorkspaceSandbox`, held in `WorkspaceSandboxRegistry`. Its relaunch primitive already
exists and is much smaller:

- `JailedWorkspaceSandboxProvisioner` is a **unit struct** and `provision(&self, spec)` is
  stateless (`workspace_tool_sandbox.rs:249-314`). Relaunching is calling it again with the same
  `WorkspaceSandboxSpec { session_id, session_dir, worktree_path }`.
- `DaemonSessionHost::provision_workspace_tool_sandbox` (`svc_ensure_session_room_for_agents.rs:214-239`)
  already does spec-build → provision → `registry.insert`. It is **the only insert** in the repo.
- `reprovision_colocated_checkout_jail` (`svc_start_sandboxed_codebase_session.rs:168-183`) is
  already the idempotent `get`-else-provision wrapper.

So the chosen auto-relaunch is *cheaper* than the option described, not bigger. The
[`2026-09-15`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md) TODO's
"`relaunch_sandboxed_runner` … reachable only from `resume_sandboxed_claude_cli_session`" is
accurate but describes the *other* jail family.

#### There is no typed "the jail is dead" signal — this is the blocking sub-problem

`WorkspaceSandbox::execute_tool` returns `ExecuteToolResponse` and a dead channel is reported as:

```rust
ExecuteToolResponse { is_error: true, error_message: message, ..Default::default() }
```

(`workspace_tool_sandbox.rs:435-444`). An ordinary failing `Shell` is shaped **identically**. The
trait's own doc comment says so deliberately (`:41-46`): *"only the dispatch is this trait's
concern, so the caller cannot tell 'the tool said no' from 'the jail said no' by the shape of the
answer alone."*

A retry keyed on `is_error` would relaunch the jail on every failed command. The trait therefore
needs a typed transport-failure signal before a retry can be written. This is the same missing
distinction as D4's on the subagent side — *a failure that is the tool's is not a failure that is
the channel's* — and both halves of the change need it.

#### The only layer that can host the retry

| Layer | Holds registry? | Holds provisioner? | Reachable from all dispatch entries? |
|---|---|---|---|
| `JailedWorkspaceSandbox::execute_tool` (`workspace_tool_sandbox.rs:411`) | no | no | yes |
| **`LocalExecTools::run_exec_tool_locally` (`local_exec_tools.rs:88-156`, jail dispatch at `:119`)** | **yes** (`workspace_sandboxes: Arc<WorkspaceSandboxRegistry>`, `:25-29`) | no — **one field to add** | **yes** |
| `ExecToolRpcHandler::execute_tool` (`tddy-daemon-rpc/src/exec_tool/ports.rs:32`) | no | no | yes |
| `DaemonSessionHost::provision_workspace_tool_sandbox` | yes | yes | **no** — hop 4 has no `DaemonSessionHost` |

`LocalExecTools` is the single choke point. `LocalExecTools::new` has exactly **one** call site
(`handler_state.rs:90-96`), from a `DaemonSessionHost` that already owns
`workspace_sandbox_provisioner` (`connection_service.rs:110-111`), so adding
`Arc<dyn WorkspaceSandboxProvisioner>` is a one-line constructor change.

Three entries reach it and all must be covered by the retry:
`tddy-daemon-rpc/src/exec_tool/ports.rs:81-84` (unary) and `:161` (streaming);
`svc_resolve_os_user/local_exec_tool_dispatch.rs:15-24`;
`svc_start_hosted_agent_clone.rs:281` (a roster agent's own turn loop — **the FastContext path**).

#### Registry mechanics for a swap

`WorkspaceSandboxRegistry` (`:508-581`) has `insert` / `get` / `remove` / `stop_all` and a `Drop`,
but **no `replace`**. A relaunch is `remove` — which drops the last `Arc`, firing
`impl Drop for JailedWorkspaceSandbox` → `stop()` → `kill` + `wait`, escalating to
`terminate_sandbox_process` (SIGTERM to `-pid`, 200 ms, then SIGKILL to `pid` and `-pid`) — then
`insert` of the new one. A `get()` clone outstanding in a concurrent call delays the teardown, so
the swap needs to be deliberate rather than incidental.

`JAIL_READY_TIMEOUT` is 120 s (`:246`), so a relaunch is not instant and the retry must be bounded
and must not be attempted concurrently by two tool calls on the same session.

#### Test gating — what can actually gate this change

**Runs on Linux CI (ungated):**

| File | Relevance |
|---|---|
| `tddy-sandbox-runner/tests/in_jail_tool_dispatch.rs` (6 tests) | host→jail `in_jail_tool_request`/`in_jail_tool_response` driven from the host relay — **the home for channel-death and retry tests** |
| `tddy-sandbox-runner/tests/in_jail_tool_dispatch_logging.rs` (1 test) | its doc states the invariant this change alters: *"A jail that lets one call pass its budget loses the channel for good"* — **must be updated, not merely extended** |
| `tddy-sandbox-runner/tests/host_relay_dispatch.rs` (6 tests) | relay dispatch + CONNECT tunnels |
| `tddy-daemon-sandbox/tests/workspace_tool_sandbox_plan_unit.rs` (11 tests) | **the only always-running coverage of `workspace_tool_sandbox.rs`** — layout/plan only, spawns no jail |
| `tddy-daemon-sandbox/tests/tool_catalog_sync.rs` | cross-checks the sandbox allowlist against the `tddy-tool-engine` catalog — **a `Read` schema change touches this** |
| `tddy-daemon-sandbox/tests/sandbox_runner_stdio_acceptance.rs` | `#![cfg(unix)]`, so it does run on Linux |

**Runs nowhere in CI** (`#![cfg(target_os = "macos")]`, and every CI job is `runs-on: ubuntu-*` —
`.github/workflows/ci.yml:35,73,148,275,318,354`): `sandbox_runner_spawn_smoke.rs`,
`sandbox_session_stdio_acceptance.rs`, `sandbox_stdio_seatbelt_acceptance.rs`, four of the seven
tests in `action_sandbox_acceptance.rs`, and — correcting the TODO's implied location —
`packages/tddy-daemon-rpc/tests/in_jail_conversation_acceptance.rs:28`. That last one is **not**
`#[ignore]`d; it compiles to an empty binary on Linux. `sandbox_runner_inspect.rs` is both
macOS-gated and `#[ignore]`d.

The practical consequence: **the gate for this change is the ungated `tddy-sandbox-runner` relay
suites plus new unit coverage, not a real jail.** A Seatbelt-backed proof can be added but cannot
be the gate.
