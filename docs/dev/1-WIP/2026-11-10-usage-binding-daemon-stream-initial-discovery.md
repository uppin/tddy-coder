# Initial Discovery: The daemon accounts every session's usage, and the Usage tab shows all of it (`#usage-binding 2/5`)

**Changeset**: [2026-11-10-usage-binding-daemon-stream.md](./2026-11-10-usage-binding-daemon-stream.md)
**Date**: 2026-11-10
**Passes**: 3

## Combined Conclusions

(Rewritten after each pass.)

- Usage is accounted in one place, `gather_session_usage` (`packages/tddy-agent-backend/src/backend/mod.rs:124`).
  It has **two** callers, not one:
  - `tddy-presenter` `usage_watcher` (`spawn_session_usage_watcher`, `include_main_agent: config.agent == "claude"`),
    spawned by `tddy-coder` `run_daemon` (`run.rs:2108`), with `claude_home: resolve_agent_home_dir()` (`$HOME`, run.rs:1586).
  - `tddy-sandbox-app` `print_token_summary` (macOS only), gated by `agent.spec.agent.transcript_usage` (`main.rs:578`).
- `coding_agent.transcript_usage: bool` lives in `CodingAgentSection` (`tddy-coding-agent-specs/src/spec.rs:249`),
  set `true` in `claude.yaml:56` and `claude-acp.yaml:56`, defaulted `false` in `slots/mod.rs:285`.
- The Claude reader (`backend/claude/transcript_usage.rs`) excludes `cache_*` on purpose (doc comment line 6) and
  does not dedupe by `message.id`. **Measured on a real transcript** (session `01a1c71c-…`): 10 assistant lines,
  6 unique `message.id`s; naive sum output 3648 vs deduped 2057; deduped input 12 vs cache_read 332 059 and
  cache_creation 43 803. The reported "In" is wrong by four orders of magnitude and "Out" is ~1.8× too high.
- Web: `useSessionUsage` (`tddy-session-web/src/useSessionUsage.ts`) subscribes to `TddyRemote.Stream` over the
  LiveKit room and reads `tokenUsageUpdated` (`remote.proto:112/119`). Only `tddy-coder` serves `TddyRemote`.
- User decisions (interview, 2026-11-10): all four sources in scope (Claude JSONL, Codex rollout JSONL, OpenCode SQLite,
  ACP `usage_update`), Claude first; **Rust implements only generic formats** (JSONL, SQLite, path templates, filters,
  field mapping, aggregation, dedupe, parent linkage) — no agent names in Rust; YAML declares each agent's binding;
  the **daemon** runs accounting for every session and serves the stream; cache read / cache write as **separate columns**;
  **show reported cost** when a source provides it.

## Exploration 1: verify the brief's findings — 2026-11-10

**Agent**: parent Grep/Read
**Scope**: the established findings in the `/yolo-pr-stack` invocation

### Sequence

1. `ls packages/tddy-coding-agent-specs/specs/` — which agent specs exist
2. Grep `transcript_usage` in `packages` (`*.rs`, `*.yaml`, `*.md`) — every consumer of the flag
3. Grep `usage|session_id|homes` in `specs/claude.yaml` — conversation id and jail home
4. Read `tddy-agent-backend/src/backend/mod.rs:100-170` — `gather_session_usage`
5. Read `tddy-sandbox-app/src/main.rs:560-600, 925-960` — second consumer
6. Grep `usage_watcher|SessionUsage|ConversationUsage` in `packages` — every usage surface
7. Read `tddy-session-web/src/useSessionUsage.ts:1-60` — web transport
8. Read `tddy-presenter/src/usage_watcher.rs:130-200` — hard-coded `agent == "claude"`
9. Grep `usage_watcher|claude_home|HOME` in `tddy-coder/src/run.rs` — where the watcher is spawned
10. Python over `~/.claude/projects/-Users-mantasi-Code-tddy-coder--worktrees-test-exploration/01a1c71c-3e12-7c82-8192-bf9584184e6a.jsonl` — duplicate `message.id` measurement
11. Grep `"id"|message_id|cache|subagents` in `backend/claude/transcript_usage.rs`; Grep `TokenUsage` in protos

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| ls | `specs/` | `tddy-coding-agent-specs` | agent-env, claude-acp, claude, codex-acp, codex, cursor, muse, opencode, oz, warp |
| Grep | `transcript_usage` | `packages` | `sandbox-app/src/main.rs:578`, `coding-agent-specs/src/spec.rs:249`, `specs/claude.yaml:56`, `specs/claude-acp.yaml:56`, `slots/mod.rs:285` |
| Grep | `usage_watcher\|SessionUsage\|ConversationUsage` | `packages` | `tddy-coder/src/run.rs`, `tddy-presenter/src/usage_watcher.rs`, `tddy-presenter/tests/{usage_watcher,session_usage_watcher}.rs`, `tddy-session-web/src/{SessionUsageTab.tsx,sessionUsage.ts,useSessionUsage.ts,SessionInspectorDrawer.tsx}`, `cypress/component/SessionInspectorUsageAcceptance.cy.tsx`, `cypress/support/rpc/usageBackend.ts` |
| Grep | `usage_watcher\|claude_home\|HOME` | `tddy-coder/src/run.rs` | `:15` import, `:1586` `resolve_agent_home_dir`, `:2108` spawn, `:2112` `claude_home: resolve_agent_home_dir()` |
| Grep | `TokenUsage` | protos | `tddy-service/proto/tddy/v1/remote.proto:112,119` |

### Inspected files

#### `packages/tddy-agent-backend/src/backend/mod.rs`

```rust
pub fn gather_session_usage(
    session_dir: &std::path::Path,
    session_id: &str,
    claude_home: &std::path::Path,
    fallback_model: &str,
    include_main_agent: bool,
) -> Vec<crate::token_accounting::ConversationRecord> {
    ...
    if include_main_agent {
        records.push(read_claude_transcript_usage(claude_home, session_id, fallback_model));
        records.extend(read_claude_subagent_usages(claude_home, session_id, fallback_model));
    }
    let accounting_path = session_dir.join("egress").join("accounting.json");
    ...
}
```

#### `packages/tddy-sandbox-app/src/main.rs`

```rust
    let model_for_summary = model.clone();
    let is_transcript_agent = agent.spec.agent.transcript_usage;
...
#[cfg(target_os = "macos")]
fn print_token_summary(session_dir, session_id, claude_home_dir, model, include_main_agent) {
    use tddy_core::backend::gather_session_usage;
```

#### `packages/tddy-presenter/src/usage_watcher.rs`

```rust
    let target = UsageWatchTarget {
        session_dir: config.session_dir,
        session_id: config.session_id,
        claude_home: config.claude_home,
        fallback_model: config.model,
        include_main_agent: config.agent == "claude",
    };
```

#### `packages/tddy-coding-agent-specs/specs/claude.yaml`

```yaml
    - { if: resume, then: ["--resume", "{session_id}"], else: ["--session-id", "{session_id}"] }
    conversation: { kind: daemon_session_id }
    transcript_usage: true
# A persistent home at <tddyhome>/homes/claude, mounted rw: ...
```

#### `packages/tddy-session-web/src/useSessionUsage.ts`

```ts
 * The presenter broadcasts the full cumulative snapshot as a `tokenUsageUpdated` `ServerMessage`
 * (never a delta); each event replaces the held snapshot.
...
  const client = useRoomClient(roomConnected ? room : null, serverIdentity, TddyRemote);
```

#### Real transcript measurement

```
assistant lines 10 unique ids 6 max dup 2
naive {'input_tokens': 20, 'output_tokens': 3648, 'cache_read_input_tokens': 543255, 'cache_creation_input_tokens': 81569}
dedup(last) {'input_tokens': 12, 'output_tokens': 2057, 'cache_read_input_tokens': 332059, 'cache_creation_input_tokens': 43803}
```

### Findings

- Every finding in the brief is confirmed, plus a second consumer (`tddy-sandbox-app`) the brief missed.
- Dedupe by `message.id` is required (Claude writes one line per content block with the same usage).
- Cache counters dominate Claude usage; excluding them misreports usage completely.

## Exploration 2: spec model, agent-name rules, web Usage tab, token record — 2026-11-10

**Agent**: Explore subagent (sonnet)
**Scope**: tddy-coding-agent-specs parsing/validation, no-agent-name tests, tddy-session-web Usage tab + Cypress pins, token_accounting, usage_watcher, code issues, todo, PRDs

### Discovery dump: usage-section planning (worktree pr-stack-accounting-tokens)

No `.codegraph/` at repo root, so I used grep and read only. All paths are under `/Users/mantasi/Code/tddy-coder/.worktrees/pr-stack-accounting-tokens`. A WIP file already exists and is untracked: `docs/dev/1-WIP/2026-11-10-usage-accounting-whole-work-initial-discovery.md`. I did not read it fully, only saw it in grep hits.

## 1. Sequence (chronological)

1. `ls -a` at repo root and `ls` of `packages/tddy-coding-agent-specs/{,src,src/slots,specs,tests,docs}`. Confirmed no `.codegraph`, and found the layout.
2. grep `transcript_usage` repo-wide, excluding node_modules, target and .git. First attempt hit a zsh glob error; I re-ran it. Purpose: find every consumer of the flag being replaced.
3. Read `packages/tddy-coding-agent-specs/src/spec.rs` in full (598 lines). Purpose: Q1 section parsing and validation.
4. Read `src/lib.rs`, `specs/claude.yaml`, `src/spec/pass_env.rs` (head), `src/spec/headless_section.rs` (head). Purpose: embedding, loading, and a validation example.
5. Read `docs/coding-agent-specs.md` (the table rows) and grepped `{placeholder}` patterns in `src`.
6. Read `packages/tddy-actions/src/tool/{template.rs,extensions.rs,run.rs,def.rs,registry.rs,files.rs}`. Purpose: the generic template mechanism and the `ExtensionOwner` load-time validation seam.
7. grep `{home}`, `{session_dir}`, `{self_exe}` across specs, sandbox and agent-backend. Purpose: all existing variable substitution sites.
8. Looked for a JSON-path or predicate language. grep for `json_path|JsonPointer|.pointer(|jsonpath` failed on a zsh glob, so I fell back to reading `ArgvItem` and `template.rs`. Result: none exists (see 1c).
9. Listed tests named like `no_agent*`, `names_no*`, `yaml_only*`, `sweep*`. Read `tddy-sandbox/tests/no_agent_vocabulary.rs`, `tddy-session-lifecycle/tests/launcher_names_no_agent.rs`, `no_agent_name_gates.rs`, `tddy-coding-subagent/tests/no_agent_names.rs`, and the head of `yaml_only_agent_acceptance.rs`. Purpose: Q2.
10. grep `builtin_specs()` and listed spec tests (`spec_load_acceptance.rs`, `harness_specs_acceptance.rs`, `spec_context_globs_acceptance.rs`). Purpose: which tests touch every spec.
11. Read `packages/tddy-agent-backend/src/token_accounting.rs` in full, `backend/claude/transcript_usage.rs` in full, and `backend/mod.rs:105-175` (`gather_session_usage`). Purpose: Q4.
12. Read `packages/tddy-presenter/src/usage_watcher.rs` in full and `tests/{usage_watcher,session_usage_watcher}.rs` in full. Then read the `run.rs:2085-2135` call site, `remote.proto:105-135`, `convert.rs:160-185`, and `sandbox-app/main.rs:560-600` and `925-975`. Purpose: Q5.
13. Read `packages/tddy-session-web/src/{SessionUsageTab.tsx,sessionUsage.ts,useSessionUsage.ts}` in full. grep `usage` in `SessionInspectorDrawer.tsx` and read lines 265-285. Read the Cypress acceptance spec, `usageBackend.ts`, and the page-object usage hits. Purpose: Q3.
14. `ls` of `docs/code-issues/` in the 5 packages, with Location, Status and Claimed-by lines. `ls docs/dev/todo`, then a usage/token grep. Purpose: Q6.
15. Read `docs/ft/coder/session-token-accounting.md` and `docs/ft/web/session-usage-inspector.md` in full. Purpose: Q7.
16. Final spot checks: the sweep changeset text, the `agent == "claude"` grep, and the `no_agent_name_gates` scan logic.

## 2. Inspected files and informative excerpts

### `packages/tddy-coding-agent-specs/src/spec.rs`

The flag to be replaced (lines 245-249):
```rust
    /// This agent leaves transcripts the host can total token usage from, when a transcript
    /// reader for its format exists. A summary that reads transcripts gates on this rather
    /// than naming the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transcript_usage: bool,
```
- The section struct is `CodingAgentSection` (line 174), `#[serde(deny_unknown_fields)]`, so a new `usage:` field must be added to it.
- Every optional section follows one pattern: `#[serde(default, skip_serializing_if = "Option::is_none")] pub headless: Option<HeadlessSection>` (lines 194-195). The same applies to `route`, `resume`, `acp` and `conversation`.
- `CodingAgentExtension` implements `ExtensionOwner` (line 36). `validate` (line 41) calls `parse_section`, then runs cross-field checks that return `Err(String)`. Example, lines 68-73:
```rust
        if agent.models.list_command.is_some() != agent.models.adapter.is_some() {
            return Err(
                "models.list_command and models.adapter are declared together or not at all"
```
- Adapter names are checked at load against build-time registries:
  - `AcpHooksAdapters::builtin().contains(name)` (line 54)
  - `LaunchAdapters::builtin()` for `models.adapter`, `resume.adapter` and `conversation.adapter`
  - `validate_headless(.., &AdapterRegistry::builtin())` (line 104)
  - A usage-format name could be refused the same way.
- `parse_section` (lines 140-160) does `serde_yaml::from_value`, then `hooks::adapter(..)`, `validate_env_entries(&agent.pass_env)` and `route_slot_errors(route)`. It wraps failures as `SpecError::Invalid { tool, message }` and prefixes the message with the section name, e.g. `format!("pass_env: {e}")`. A new `usage_errors(..)` would slot in beside `route_slot_errors`.
- `SpecError` has variants `NotACodingAgent`, `NoRunSection`, `Invalid` (lines 163-171).
- Closed-vocabulary words like capabilities use a serde enum, so an unknown word is refused at load (`capability.rs`, field doc at spec.rs:220-224).
- Existing in-file validation tests (`extension_validation_tests`, lines 499-598) mutate a section YAML and assert the exact `Err(String)`. `placement_tests` load embedded specs via `crate::WARP_SPEC_YAML`.

### Where `usage:` sits in the YAML

- The agent section is `extensions.coding_agent`, not top-level.
- `claude.yaml:33-34` has `extensions:` then `coding_agent:`, and `claude.yaml:56` has `transcript_usage: true`. The same line is at `claude-acp.yaml:56`.
- `ToolDef` (`packages/tddy-actions/src/tool/def.rs:20`) is `#[serde(deny_unknown_fields)]`. Its top-level keys are `schema_version, id, summary, binary, deps, reads, run, extensions, network, home, files, secrets, policy`.
- `extensions: BTreeMap<String, serde_yaml::Value>` is "Owner-validated sections ... `tddy-actions` never reads one".
- So a `usage:` section would go under `extensions.coding_agent.usage`, unless `ToolDef` itself is changed (unlikely, since tddy-actions is agent-agnostic).

### Embedding and loading (`src/lib.rs`)

- `pub const CURSOR_SPEC_YAML ... include_str!("../specs/cursor.yaml")`, and the same for OPENCODE, MUSE, WARP, CODEX, OZ, CLAUDE_ACP and CODEX_ACP (lines 55-77).
- Claude is embedded inline inside `builtin_specs()` (lines 80-119). It is a `Vec<(path, yaml)>` of 9 entries: claude, cursor, opencode, muse, warp, codex, oz, claude-acp, codex-acp.
- `agent-env.yaml` is separate: `agent_env_base.rs` with `AGENT_ENV_BASE_YAML`.
- `embedded_spec(id)` (line 127) parses with `ToolDef::parse` then `CodingAgentSpec::of`.
- `load_agent_registry(overrides_dir)` (line 146) runs `ToolRegistry::from_embedded(BUILTIN_TOOLS ++ BUILTIN_SUPPORT_TOOLS ++ builtin_specs())`, then `ToolRegistry::load_with_owners(builtin, overrides_dir, &[&CodingAgentExtension])`.
- Operator overrides at `<tddyhome>/tools/<id>.yaml` replace a built-in whole.

### Load-time refusal plumbing (`packages/tddy-actions/src/tool/registry.rs:153-172`)

- `validate_extensions` runs for every tool and every extensions key. An unowned key is `ToolError::UnknownExtension`. An owner's `Err(message)` becomes `ToolError::ExtensionRejected { file, owner, message }`, which names the file.
- This is the load-time refusal seam.

### Existing template and variable mechanism (1b)

There are three separate ad-hoc mechanisms, none of them generic across sections.

**A. argv templates** (`packages/tddy-actions/src/tool/{template.rs,run.rs}`)
- `ArgvItem` (run.rs:50) is untagged: `Lit(String)`, `When { if_, then, else_ }` and `Splice { splice }`. The doc says "A structured template, not a string mini-language".
- `substitute()` (template.rs:102) replaces `{identifier}`. It is the only substitution routine. `is_identifier` allows `[A-Za-z0-9_]`, and braces that don't enclose an identifier stay literal.
- `{binary}` is special-cased. Any other name is looked up in `TemplateParams.values`.
- An unbound name gives `ToolError::UnboundPlaceholder`, never an empty entry.
- The `if:` here is only "param is set (bound non-empty)". It is a presence test, not a predicate over data.

**B. `{home}` in placed MCP paths** (`slots/cursor.rs:83-93`)
- A bare `path.replace("{home}", ..)`.
- Refused with `SlotError::UnboundHome` when `{home}` is named and no home is bound (`slots/mod.rs:109-110`).
- `SlotInputs.home` (`slots/mod.rs:63`) is "what `{home}` resolves to: the jail's persistent home or the host's `$HOME`".

**C. Other placeholder sites**
- Jail `overlay` values: `{home}` and `{tmp}` (`tddy-actions/src/tool/files.rs:30`, replaced in `tddy-sandbox-recipes/src/tool_grants.rs:286`).
- `headless.env` values: `{session_dir}` and `{self_exe}`.
  - `tddy-agent-backend/src/adapters/headless.rs:265-283`.
  - It refuses a value naming `{session_dir}` when the request has none.
  - A quoted doc comment says: "The values a spec's `headless.env` may name: `{session_dir}` and `{self_exe}`".
- Codex spec: `TDDY_CODEX_OAUTH_OUT: "{session_dir}/codex_oauth_authorize.url"`.

**There is no `{session_id}` or `{home}` substitution for filesystem path templates in a reusable function.** `{session_id}` is only an argv param. Usage path templates (`{home}/.claude/projects/*/{session_id}.jsonl`) would need a new small substituter. It could reuse the `is_identifier` and unbound-refusal conventions.

### JSON field-path or predicate mini-language (1c)

- None found. The only `if:` is `ArgvItem::When` (param presence).
- The existing readers navigate JSON in Rust with `serde_json::Value` indexing: `value.get("type")`, `message["usage"]["input_tokens"]`.
- `ModelsSection` and `adapters` are named Rust adapters selected by string, not declarative paths.
- The usage filter and mapping language must therefore be new. At most it can borrow the `{ if: .., then: .. }` YAML flavor and the "unknown word refused at load" pattern.

### Specs tests that touch every spec or the `coding_agent` section (1d)

- `tests/spec_load_acceptance.rs`: Claude override and unknown-slot/extension refusals.
  - `a_claude_override(mcp, extra_extensions)` at line 40 builds an override YAML.
  - `the_builtin_claude_spec_loads_and_depends_on_tddy_tools` at line 67.
- `tests/harness_specs_acceptance.rs`: per-agent opencode, muse and warp assertions, plus `the_builtin_registry_holds_opencode_muse_and_warp_beside_claude_and_cursor` at line 136.
- `tests/spec_context_globs_acceptance.rs`: per-agent `context_globs`.
- Others: `tests/agent_registry_unit.rs`, `cursor_spec_acceptance.rs`, `cli_spec_features_unit.rs`, `acp_slots_unit.rs`, `pass_env_schema.rs`, `route_body_drift_acceptance.rs`, `headless_*_golden.rs`.
- I did not find a test that snapshots every spec's whole `coding_agent` section.
- Per-spec tests would only break if `deny_unknown_fields` rejects a new key in an old fixture.
- Outside the crate, these load `builtin_specs()` or `embedded_spec`:
  - `tddy-session-split/src/agent_argv.rs`
  - `tddy-tools-cli/tests/list_models_tools_dir.rs`
  - `tddy-agent-launch/src/cli_agent_steps/cli_agent_steps_tests.rs`
  - `tddy-sandbox-app/src/main.rs`
  - `tddy-daemon/tests/split_session_resume_acceptance.rs`
  - `tddy-session-lifecycle/tests/{yaml_only_agent_acceptance,claude_cli_from_spec_acceptance,claude_cli_fullscreen_renderer,agent_registry_support/mod,claude_jail_support/mod}.rs`
- `yaml_only_agent_acceptance.rs` rewrites claude.yaml with a text `replace` on `"capabilities: [hooks, "`. Do not reformat that line.
- `slots/mod.rs:285` has a literal `CodingAgentSection { ..., transcript_usage: false, ... }` struct initializer, a test fixture. Any new field there needs an update. A `Default` for `usage` or removing the bool must be handled.
- Every consumer of `transcript_usage`:
  - `packages/tddy-sandbox-app/src/main.rs:578`, `let is_transcript_agent = agent.spec.agent.transcript_usage;`
  - `spec.rs:249`
  - `slots/mod.rs:285`
  - `specs/claude.yaml:56`
  - `specs/claude-acp.yaml:56`
  - `docs/coding-agent-specs.md:67`
  - the changesets, and the sweep changeset `docs/dev/changesets/2026-11-06-agent-tools-sweep-yaml-only.md:16,21`

### Doc table rows in `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (1e)

- The top table (lines 14-~52) has `| Module | Holds |` rows per source file.
- The `spec.rs` row lists the submodules (`spec/headless_section.rs, acp.rs, slot_shapes.rs, route_slot.rs, pass_env.rs, argv.rs`).
- A new `spec/usage.rs` would need a row there.
- Row for `headless.rs`.
- Rows for `specs/*.yaml` per agent. Those rows are long.
- Rows for `registry.rs` and `capability.rs`, with the `#agent-tools N/19` annotations.
- Section "The only home of agent metadata (the sweep, `#agent-tools` 20/28)" has a `| Metadata | Spec home |` table. Line 67:
```
| permission modes, transcript usage | `coding_agent.permission_modes`, `coding_agent.transcript_usage` |
```
  This is the row that must change.
- Later sections: "Which transport a workflow turn takes", and "The headless protocol" with a `| Field | Values | Absent |` table, the model for a usage-field table.
- The doc says: "No package depends on this crate to learn about an agent. Specs are injected as plain data by binary and composition-root modules". See rule 2.

### `packages/tddy-agent-backend/src/token_accounting.rs` (Q4)

```rust
pub struct TokenUsage { pub input_tokens: u64, pub output_tokens: u64 }   // Copy, Default, Serialize...
impl TokenUsage { pub fn total(&self) -> u64 { self.input_tokens + self.output_tokens } }
impl std::ops::Add for TokenUsage { ... }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub agent: String,
    pub id: String,
    pub model: String,
    #[serde(rename = "inputTokens")]  pub input_tokens: u64,
    #[serde(rename = "outputTokens")] pub output_tokens: u64,
    #[serde(rename = "totalTokens")]  pub total_tokens: u64,
    pub turns: u32,
}

pub fn format_token_summary(session_id: &str, records: &[ConversationRecord]) -> String {
    let mut lines = vec![format!("Session token usage (session {session_id}):")];
    ...
        lines.push(format!(
            "- {} [{}] [{}]: in={} out={} total={} turns={}",
            r.agent, r.id, r.model, r.input_tokens, r.output_tokens, r.total_tokens, r.turns
        ));
    ...
    lines.push(format!("TOTAL: in={total_in} out={total_out} total={total}"));
```
- There is no parent, child or depth field. The record is flat, with 7 fields.
- The module doc says: "Agent-specific token *sources* live with their backend, not here".
- `cache_*` counters are excluded on purpose.
- The record is wire-shared by three consumers:
  - the `subagent_list` MCP tool
  - the in-jail `egress/accounting.json` (`{"conversations":[...]}`)
  - the proto `ConversationRecord` (`tddy-service/proto/tddy/v1/remote.proto:119-127`, 7 fields, comment `mirrors tddy_core::token_accounting::ConversationRecord`)
- Adding a field means changing all of these: the Rust struct, the proto, `tddy-service/src/convert.rs:167-181`, the TS `ConversationUsage`, and the generated `remote_pb`.
- Tests: `tests/token_accounting_red.rs` (`TokenUsage` arithmetic) and `tests/token_accounting_acceptance.rs` (`renders_a_per_agent_breakdown_with_a_total_row`).

### Current Claude readers (`backend/claude/transcript_usage.rs`), the logic the generic formats must reproduce

- `sum_assistant_usage` walks JSONL lines, keeps `type == "assistant"`, sums `message.usage.input_tokens` and `output_tokens`, tracks the last `message.model`, and counts one turn per such line.
- Unparseable lines are skipped.
- Main transcript path: `<claude_home>/.claude/projects/<each project dir>/<session_id>.jsonl`. It iterates all project dirs and sums every match.
- Subagents: `<claude_home>/.claude/projects/*/<session_id>/subagents/agent-<id>.jsonl`.
  - Sibling `agent-<id>.meta.json` gives `agentType` for `agent`, falling back to `"subagent"`.
  - The id is the file stem.
  - Results are sorted by id.
- A missing transcript yields a zero row with the fallback model, and the agent is the literal `"claude"` (line 73).
- There is no dedupe by message id. The WIP doc notes the readers exclude `cache_*`.
- Parent-child linkage today is implicit in the path (`<session_id>/subagents/`). It is not carried in the record.

### `gather_session_usage` (`backend/mod.rs:105-175`)

- Signature: `(session_dir, session_id, claude_home, fallback_model, include_main_agent)`.
- It pushes the main-agent row and the subagent rows if `include_main_agent`, then extends from `<session_dir>/egress/accounting.json`.
- It is re-exported from `tddy_agent_backend::backend`.
- Callers use the old path name `tddy_core::...` (the crate was renamed; `tddy_core::usage_watcher` is still imported in `tddy-coder/src/run.rs:15`).

### `packages/tddy-presenter/src/usage_watcher.rs` and tests (Q5)

- Polling model.
  - `spawn_usage_watcher(target, poll_interval, event_tx)` runs `tokio::time::interval`.
  - Each tick, `tokio::task::spawn_blocking(gather_session_usage(...))`.
  - The result goes to `emitter.emit_if_changed(records)`.
  - The task is aborted by the owner. Dropping subscribers does not stop it.
  - The module doc says: "Polling (not `notify`/inotify) keeps this dependency-free and robust to the several files a session appends to."
- `SessionUsageEmitter { event_tx: broadcast::Sender<PresenterEvent>, last: Option<Vec<ConversationRecord>> }`.
  - `emit_if_changed` returns false when `self.last.as_ref() == Some(&records)`.
  - Otherwise it sends `PresenterEvent::TokenUsageUpdated(records.clone())` even with no subscribers, then stores `last`.
  - The first call always emits.
  - The snapshot is full and cumulative, never a delta.
- `UsageWatchTarget { session_dir, session_id, claude_home, fallback_model, include_main_agent }`.
- `SessionUsageWatchConfig { session_dir, session_id, claude_home, agent: String, model, poll_interval }`.
- The Rust agent-name literal (usage_watcher.rs:146-157):
```rust
        include_main_agent: config.agent == "claude",
```
  The doc comment says "The main Claude agent ... is included only for Claude sessions; other agents (e.g. Cursor) report their usage solely through the tddy accounting file".
- Known TODO in the module doc (lines 20-23): snapshot-on-connect replay is not yet done. A comment in `emit_if_changed` says "a later subscriber replays the current snapshot on connect". The PRD says the daemon emits on connect. The code only has `last` change-detection.
- Wiring: `packages/tddy-coder/src/run.rs:2108-2125` calls `spawn_session_usage_watcher` only when `livekit_enabled` and `usage_event_tx` is `Some`.
  - `session_dir: session_artifact_dir`.
  - `session_id: args.session_id...unwrap_or_default()`.
  - `claude_home: resolve_agent_home_dir()`.
  - `agent: agent_str.to_string()`.
  - `model` from args or `default_model_for_agent`.
  - `poll_interval: SESSION_USAGE_POLL_INTERVAL`.
  - It is aborted via the shutdown flag.
- The event is `PresenterEvent::TokenUsageUpdated(Vec<ConversationRecord>)` (`presenter/presenter_events.rs:54`).
  - Converted in `tddy-service/src/convert.rs:167` field by field.
  - `tddy-tui/src/virtual_tui.rs:746` ignores it.
- Test pins, `tests/usage_watcher.rs` (3 tests, emitter only):
  - `emits_the_current_snapshot_the_first_time_the_watcher_gathers`
  - `re_emits_the_new_snapshot_after_usage_grows`
  - `does_not_re_emit_an_unchanged_snapshot`
  - They use `try_recv`, ids like `claude-main`, and construct `ConversationRecord` literals with 7 fields. Adding a field to the record breaks these literals.
- Test pins, `tests/session_usage_watcher.rs` (2 tokio tests, 20 ms poll, 2 s timeout):
  - `broadcasts_the_current_claude_usage_snapshot_for_a_claude_session`. A transcript line `{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":120,"output_tokens":30}}}` under `home/.claude/projects/proj/<sid>.jsonl` gives exactly one record `{agent:"claude", id:"sess-1", model:"claude-opus-4-8", in:120, out:30, total:150, turns:1}`.
  - `omits_the_claude_main_agent_for_a_cursor_session`. A stray Claude transcript plus `egress/accounting.json` (`{"conversations":[{"agent":"fastcontext","id":"fc-1","model":"ollama","inputTokens":5,"outputTokens":1,"totalTokens":6,"turns":1}]}`) with `agent: "cursor"` gives only the `fastcontext` record.
  - They pin the `SessionUsageWatchConfig` field names including `agent: String` and `claude_home`. Both tests are agent-name-driven (`"claude"`, `"cursor"`), so a spec-driven design changes this API.
- The only other `"claude"` literal hits in presenter and backend `src` were `agent_session_runner.rs` and `presenter_impl.rs`; I did not inspect them.

### Web Usage tab (Q3)

**`SessionUsageTab.tsx`**
- Columns, in order: Agent, Model, In, Out, Total (right-aligned), Turns (right-aligned).
- Footer row: "TOTAL", empty model cell, sums In, Out, Total, empty Turns.
- Rows are `usage.map(c => <TableRow key={c.id} ...>)`, a flat list with no indentation, depth or parent. Duplicate ids would collide on the React key.
- Test ids:
  - panel: `sessions-usage-tab-panel`
  - empty state: `sessions-usage-empty`, text "No token usage reported yet."
  - stream error: `sessions-usage-stream-error`, text `Usage unavailable: ${error}`
  - row cells: `sessions-usage-row-{,agent-,model-,input-,output-,total-,turns-}${c.id}`
  - totals: `sessions-usage-total-{input,output,total}`
- Numbers go through `formatTokens` (`src/formatTokens.ts`, with a test). `turns` is `String(c.turns)`.
- The tab owns the subscription, so the stream opens only while the tab is mounted.
- An error with zero rows shows only the error. An error with rows keeps the last snapshot readable.

**`sessionUsage.ts`**
```ts
export interface ConversationUsage {
  agent: string; id: string; model: string;
  inputTokens: bigint; outputTokens: bigint; totalTokens: bigint; turns: number;
}
export function emptyUsage(): ConversationUsage[]
export function usageTotals(records)   // sums bigint in/out/total
```
There is no parent or depth field. Tests are in `sessionUsage.test.ts`.

**`useSessionUsage.ts`**
- Uses `useRoomStatus`, `useRoomClient(roomConnected ? room : null, serverIdentity, TddyRemote)`.
- Enqueues one empty `ClientMessage` to open `TddyRemote.Stream`.
- On `serverMessage.event.case === "tokenUsageUpdated"` it replaces the snapshot with the 7 proto fields mapped.
- It maps explicitly field by field (lines 88-96), so a new proto field needs the map updated.
- A catch sets `error = rpcFailureText(err)`.

**`SessionInspectorDrawer.tsx`**
- Imports `SessionUsageTab` (line 23).
- Lines 278-281: `tab === "usage" ? <ScrollArea ...><SessionUsageTab room={room} serverIdentity={serverIdentity} /></ScrollArea>`.
- `InspectorTab` and `InspectorTabs` come from `./InspectorTabs`. The tab is selected via the URL param `PARAM_INSPECTOR`.
- Line 89 has a prop doc: "LiveKit participant identity of the daemon/presenter side, for the token-usage stream".

**`cypress/component/SessionInspectorUsageAcceptance.cy.tsx`** (5 tests)
1. A Usage tab exists in the inspector tab strip.
2. One row per conversation with agent, model and formatted numbers. Fixtures:
   - CLAUDE `claude-main`: 12340 in, 3210 out, 15550 total, 7 turns. Expected "12,340", "3,210", "15,550", "7".
   - EXPLORE `agent-01`: 4100 in, 820 out, 4920 total, 2 turns, model `claude-haiku-4-5`.
3. TOTAL row: 16,440 / 4,030 / 20,470.
4. A second snapshot replaces totals: 24,100 in, 29,920 total. The backend yields two snapshots and the view shows the latest.
5. Zero state: `usageEmpty` exists and `usageRow("claude-main")` does not.
   - It drives the full `SessionsDrawerScreen` through `mountWithRpc` and `aSessionUsageBackend`, with no `cy.intercept`.
   - It uses `page.drawerItem`, `inspectorUsageTab`, `usageRow*` and `usageTotal*` from `cypress/support/pages/sessionsDrawerPage.ts` (lines ~304-330). These bind `TEST_IDS.sessionsUsage*` and `sessionsUsageRow*` helpers.
- It pins flat rows keyed by id with no hierarchy, so nesting would be new. Adding a nested/indent presentation must keep these test ids and texts.

**`cypress/support/rpc/usageBackend.ts`**
- `ConversationRecordInit { agent, id, model, inputTokens: bigint, outputTokens: bigint, totalTokens: bigint, turns: number }` and `aConversationRecord(overrides)`. Defaults are claude, `claude-main`, `claude-opus-4-8`, zeros.
- `aSessionUsageBackend(sessions, snapshots)` implements `TddyRemote.stream` as an async generator yielding `create(ServerMessageSchema, { event: { case: "tokenUsageUpdated", value: { conversations } } })` per snapshot.
- A new proto field (parent or depth) must be added to the init type and defaults here.

### Agent-name rules in Rust (Q2): see Finding 2

### PRDs (Q7): see Finding 7

## 3. Grep and glob log

| Pattern | Scope | Notable hits |
|---|---|---|
| `transcript_usage` | repo, excluding node_modules, target, .git | sandbox-app `main.rs:578`; specs `spec.rs:249`, `slots/mod.rs:285`, `claude.yaml:56`, `claude-acp.yaml:56`, `coding-agent-specs.md:67`; agent-backend `claude/transcript_usage.rs`, `backend/mod.rs:20,116,141`, `claude.rs:6-7`; docs/ft `session-usage-inspector.md:62`, `session-token-accounting.md:47`; changesets listed in Finding 1d; WIP discovery doc |
| `{name}` placeholders / `{home}` / `{session_dir}` / `{self_exe}` / `{tmp}` | `tddy-coding-agent-specs/src`, sandbox, agent-backend | `slots/cursor.rs:83-93`, `slots/mod.rs:63-110`, `tool_grants.rs:286`, `adapters/headless.rs:265-283`, `files.rs:30` |
| `render_argv\|ArgvItem\|ExtensionOwner\|substitute` | `tddy-actions/src/tool` | `template.rs:69,102`, `run.rs:50`, `extensions.rs:8`, `registry.rs:81-87,153` |
| `json_path\|JsonPointer\|.pointer(\|jsonpath` | packages | errored on a zsh glob; no usable result. Absence of a mini-language concluded from reading `ArgvItem` and the readers |
| `sweep\|yaml-only\|names no agent` | `packages/*/tests`, `scripts` | list of the no-agent tests below |
| `must not name\|names no agent...` | `packages/*/tests`, `src` | errored on a zsh glob |
| `builtin_specs()` | packages | 10 files, listed in Finding 1d |
| `TokenUsageUpdated` | packages | `presenter_events.rs:54`, `usage_watcher.rs`, `convert.rs:167`, `remote.proto:112,119`, `virtual_tui.rs:746` |
| `spawn_session_usage_watcher\|SessionUsageWatchConfig` | packages | only `tddy-coder/src/run.rs:15,2108` outside presenter |
| `Claimed by` | the 5 packages' `docs/code-issues` | none in those 5 packages (the hits were other packages) |
| `token_accounting\|usage_watcher\|SessionUsageTab\|transcript_usage\|token usage\|...` | `docs/dev/todo` | no relevant hits |
| `agent == "claude"` | presenter, agent-backend, coder `src` | only `tddy-presenter/src/usage_watcher.rs:155` |

## 4. Findings, numbered to match the questions

### 1. Spec model

**1a. Parsing.**
- Sections are fields on `CodingAgentSection` (`spec.rs:174-256`) under `extensions.coding_agent`.
- The struct has `deny_unknown_fields`.
- Optional sections use `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- Parsing is `serde_yaml::from_value` in `parse_section` (`spec.rs:140`).
- Submodules under `spec/` hold section types: `acp.rs`, `argv.rs`, `headless_section.rs`, `pass_env.rs`, `route_slot.rs`, `slot_shapes.rs`. The `slots/` dir holds renderers, not schema. A new `spec/usage.rs` fits the existing convention.
- A top-level YAML key outside `extensions` would be refused by `ToolDef`'s `deny_unknown_fields`.

**1b. Generic template or variable mechanism: partial and fragmented.**
- Only `ArgvItem` (`{param}` in argv literals, `TemplateParams`) is generic. It is bound to argv rendering and `ResolvedTool`.
- `{home}`, `{session_dir}`, `{self_exe}` and `{tmp}` are each replaced ad hoc in their own sites.
- There is no `{session_id}` or `{home}` path-template helper.
- Reusable conventions: `is_identifier` brace parsing (leaving non-identifier braces literal), and "an unbound placeholder is an error naming it, never an empty value" (`ToolError::UnboundPlaceholder`, `SlotError::UnboundHome`).

**1c. Load-time refusal: well established.**
- The `ExtensionOwner::validate(tool_id, &Value) -> Result<(), String>` seam, which the registry wraps as `ExtensionRejected { file, owner, message }`.
- Serde closed enums are refused with the file and the word named.
- Adapter-name registries are checked in `validate`.
- `*_errors(&section) -> Result<(), String>` helpers (`route_slot_errors`, `validate_env_entries`), with messages like `"env entry '{entry}' would admit a TDDY_* variable..."`.
- Tests assert exact `Err(String)` via `CodingAgentExtension.validate("opencode", &section)` after mutating the YAML (`spec.rs:499-598`).

**No JSON-path or predicate mini-language exists to reuse.** `ArgvItem::When` tests presence of a param, not data. A filter and mapping language would be new.

**1d. Embedding, loading and tests.**
- Embedding and loading are as above. `include_str!` is in `lib.rs`, and the Claude yaml is inline in `builtin_specs()`.
- The list of specs tests is above. Nothing snapshots all specs' `coding_agent` section, but `slots/mod.rs:285` has a full struct literal.
- Back-compat to decide on: removing `transcript_usage` breaks old operator overrides under `deny_unknown_fields` (a claude.yaml override with `transcript_usage: true` would fail at load).
- Also check `a_claude_override` in `spec_load_acceptance.rs:40` for hard-coded sections.

**1e. Doc rows.** `docs/coding-agent-specs.md` needs:
- the line-67 row to change from `transcript_usage` to `usage`
- a new `spec/usage.rs` mention in the `spec.rs` row
- ideally a `| Field | Values | Absent |` table like "The headless protocol"
- spec rows for claude and others if they gain `usage:`

### 2. Rust must not name agents

There are source-scan tests of this kind. None covers `tddy-agent-backend` or `tddy-presenter` for bare `"claude"` literals. The doc rule is "nothing in production names an agent's binary, argv, models or capabilities outside `specs/*.yaml`".

- `packages/tddy-sandbox/tests/no_agent_vocabulary.rs`. It covers `tddy-sandbox/src` and `tddy-sandbox-recipes/src`, comments included. The scan is substring and case-blind.
```rust
const AGENT_NAMES: &[&str] = &["claude", "cursor", "opencode", "muse", "codex", "warp"];
```
  The doc says "neither `tddy-sandbox` nor `tddy-sandbox-recipes` has any reason to spell an agent's name... comments included, since a comment naming an agent is where the next agent arm gets added." `oz` is excluded.
- `packages/tddy-coding-subagent/tests/no_agent_names.rs`. It covers production code up to the first `#[cfg(test)]`, skipping comments.
```rust
const AGENT_ID_LITERALS: &[&str] = &["\"claude-acp\"","\"codex-acp\"","\"opencode\"","\"claude\"","\"cursor\"","\"codex\"","\"oz\""];
const AGENT_TABLES: &[&str] = &["CLAUDE_ACP","CODEX_ACP","OPENCODE","ACP_HARNESSES","HEADLESS_HARNESSES","CLAUDE_NATIVE_TOOLS","OPENCODE_NATIVE_TOOLS"];
```
- `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs`. It scans these crates' `src`:
  - tddy-agent-backend
  - tddy-agent-launch
  - tddy-coder
  - tddy-daemon-rpc
  - tddy-session-activity
  - tddy-session-lifecycle
  - tddy-worktree-service
  - It flags only a session type literal (`"claude-cli"`, `"cursor-cli"`, `"opencode-cli"`, `"muse-cli"`, `"warp-cli"`) together with a comparison (`==`, `!=`, `matches!`, `=>`, `" |`, `| "`).
  - The bare `"claude"` in `usage_watcher.rs:155` is in tddy-presenter, which is not in the list anyway.
- Other tests:
  - `tddy-session-lifecycle/tests/launcher_names_no_agent.rs` (daemon launcher names no OpenCode)
  - `tddy-supervisor/tests/supervisor_names_no_coding_agent.rs`
  - `tddy-telegram-control/tests/telegram_reads_no_agent_config.rs`
- The existing violations the usage work would remove:
  - `tddy-agent-backend/src/backend/claude/transcript_usage.rs` (a Claude-specific module with `agent: "claude"`)
  - `tddy-presenter/src/usage_watcher.rs:155`, `include_main_agent: config.agent == "claude"`
  - `backend/mod.rs` (`gather_session_usage` naming Claude readers)
- The no-name tests would apply to a new generic Rust usage module if it lives in `tddy-sandbox*` or `tddy-coding-subagent`. A new module in `tddy-agent-backend` is only constrained by the session-type scan.
- Rule from the doc (`coding-agent-specs.md`): "Protocol adapters (stream parsers, hook-file writers) stay as Rust, selected by the name a spec gives them; they own no metadata." Specs are injected as plain data, and "No package depends on this crate to learn about an agent". `tddy-presenter` and `tddy-agent-backend` cannot depend on the specs crate. `tddy-agent-backend` is a dependency of the specs crate (`spec.rs` imports `tddy_agent_backend::adapters`), so the generic usage reader types (`UsageSpec`) would have to live in `tddy-agent-backend` or lower and be injected as data. The changeset says "No package gained a compile dependency on `tddy-coding-agent-specs`" (developer direction, 2026-11-06), and `tddy-presenter` receives a `default_models` map as plain data.

### 3. Web Usage tab

Covered under Section 2. Summary: a flat table of 6 columns plus a TOTAL footer, `ConversationUsage` with 7 fields, no parent or depth, and the Cypress pins as above.

### 4. Token accounting

Covered under Section 2. Flat record with no parent, child or depth. The `ConversationRecord` struct is wire-shared by the in-jail `accounting.json` and the proto.

### 5. Usage watcher

Covered under Section 2.

### 6. Code issues and todo

**Code-issue records in the five packages: none touch `token_accounting.rs`, `usage_watcher.rs`, `SessionUsageTab.tsx` or `spec.rs`.** No file in those five directories carries a `Claimed by:` line (the `Claimed by` hits were all in other packages, and each of the five packages' records reads `Open — unclaimed` or similar). Listing by package:

- `tddy-coding-agent-specs`
  - `complexity-slots-mod-render.md`. `Location: slots/mod.rs:101 render`, 65 lines. Open, unclaimed. It is the only one near the touched files (same `slots/mod.rs` that has the `transcript_usage: false` literal at 285).
- `tddy-agent-backend`
  - `complexity-claude-process-ndjson-stream.md` (stream/claude.rs:163, unclaimed)
  - `complexity-cursor-process-cursor-stream.md` (stream/cursor.rs:122, unclaimed)
  - `complexity-headless-driver-run-turn.md` (unclaimed)
  - `complexity-spec-headless-invoke.md` (partially fixed 2026-11-07 by #829)
  - `complexity-spec-headless-turn-read-read-turn.md` (deferred to the #agent-tools sweep node)
  - `oversized-file-backend-mod.md`. `backend/mod.rs`, 537 production lines vs budget 500, "unclaimed". This is the file holding `gather_session_usage` (about lines 112-155), so new code there worsens it.
  - `oversized-file-headless-driver.md` (regressed 2026-11-05, deferred to the sweep node)
  - `oversized-file-stub.md` (unclaimed)
- `tddy-presenter`
  - `complexity-presenter-impl-activity-capture-agent-activity.md`
  - `complexity-presenter-impl-poll-workflow.md`
  - `complexity-presenter-impl-questions-send-clarification-answers.md`
  - `complexity-presenter-impl-view-channels-handle-intent.md`
  - `complexity-presenter-impl-workflow-run-try-handle-start-slash-line.md`
  - `complexity-workflow-runner-handle-elicitation.md`
  - `complexity-workflow-runner-run-start-goal-without-output-dir.md`
  - `complexity-workflow-runner-run-workflow.md`
  - `oversized-file-workflow-runner.md`
  - All are about `presenter_impl` and `workflow_runner`, not `usage_watcher.rs`, and all are unclaimed or partially fixed.
- `tddy-session-web`
  - `oversized-file-session-main-pane.md` (`SessionMainPane.tsx`, unclaimed). Not the usage files.
- `tddy-daemon`
  - `complexity-mount-host-build-session-host.md` (`runtime/mount_host.rs:227`, deferred by the user 2026-11-10, #876 sweep, unclaimed)

**`docs/dev/todo/` entries about usage, token, accounting, transcript or cost.** The grep for `token_accounting`, `usage_watcher`, `SessionUsageTab`, `transcript_usage`, token usage and cost found nothing about this feature. Two filename hits are unrelated to token usage:
- `2026-10-23-project-home-git-push-remote-and-token.md` (git token)
- `2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md` (mentions subagent transcripts)

Possibly adjacent, not read:
- `2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md`
- `2026-08-30-the-agents-tab-tree-can-only-see-subagents-the-browser-already-lists.md`
- `2026-08-30-agent-conversation-tabs-four-follow-ups.md`
- `2026-11-10-sweep-deferred-function-extractions.md`

Related TODOs found in code:
- `usage_watcher.rs:20-23` has a TODO for snapshot-on-connect replay.
- `spec.rs:44` has a TODO for `acp-agents`, unrelated.

`packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` and the existing WIP discovery doc are the most relevant planning inputs.

### 7. PRDs

- `docs/ft/coder/session-token-accounting.md`. A `tddy-sandbox-app` session accounts tokens per conversation and prints a breakdown to stderr at the end. Sources:
  - the main `claude` agent's transcript JSONL (`<claude_home>/.claude/projects/<encoded-cwd>/<session_id>.jsonl`, summing `message.usage` input and output, `cache_*` not folded)
  - Claude's nested Task subagents (`<session_id>/subagents/agent-<id>.jsonl` plus `.meta.json` `agentType`)
  - tddy `subagent_*` conversations (including Ollama), via `subagent_list` and `TDDY_TOOLS_ACCOUNTING_FILE` writing `egress/accounting.json`
  - Tokens only, no cost. Scope is the RPC/backend layer plus the stderr summary. Web and TUI are out of scope. The doc notes the live Ollama end-to-end summary has not been exercised.
- `docs/ft/web/session-usage-inspector.md` (Status: Planned, Updated 2026-07-12). A **Usage** tab in the Session Inspector showing a live per-conversation breakdown (agent, model, in, out, total, turns, plus a TOTAL row).
  - It rides the existing `TddyRemote.Stream` `ServerMessage` oneof as `TokenUsageUpdated = 13`, carrying the full cumulative `ConversationRecord` snapshot, never a delta. The client replaces its snapshot on each event.
  - The daemon should emit a snapshot on connect and on every change, from a watcher merging the three sources via `gather_session_usage`. Emissions are debounced. A missing transcript gives a zero row.
  - The layout shows flat rows (no nesting). Out of scope: USD cost, cache-token accounting, persisted history.
  - The PRD's "snapshot on connect" is not implemented (see the TODO in `usage_watcher.rs`).

### Design-relevant observations for the parent

- The current record is flat. Parent-child linkage must be added to `ConversationRecord`, the proto (`remote.proto` `ConversationRecord`, fields 1-7 used), `convert.rs`, the TS types, the `useSessionUsage` mapper, the Cypress fixtures, and the five presenter test literals.
- `accounting.json` is written by `tddy-tools` with the same camelCase shape, so a new optional field needs `#[serde(default)]` to stay compatible.
- The usage spec types must sit below `tddy-presenter` and `tddy-sandbox-app` in the dependency graph. Because those crates don't depend on the specs crate, the spec is injected as data, as `default_models` is. The specs crate already depends on `tddy-agent-backend`, so the generic reader and its serde types most naturally live in `tddy-agent-backend`, with the YAML shape parsed in `spec/usage.rs`.
- The presenter currently takes `agent: String` and compares it to `"claude"`. A spec-driven approach replaces `agent`, `claude_home` and `include_main_agent` with an injected usage spec plus a home and session context.
- Today's Claude behaviours to preserve in the generic engine:
  - `type == assistant` filter
  - sum of `message.usage.{input,output}_tokens`
  - last `message.model`
  - one turn per matching line
  - all project dirs iterated
  - `agentType` from a sibling `.meta.json`
  - subagent id from the file stem
  - sort by id
  - zero-row fallback with the fallback model
  - no dedupe by message id yet (a requested "dedupe by key" feature is new)
- Operator overrides: `<tddyhome>/tools/<id>.yaml` replace the built-in whole, so a stale override carrying `transcript_usage:` will be refused once the field is removed.
## Exploration 3: daemon spawn shapes, homes, RPC surface, drivers — 2026-11-10

**Agent**: Explore subagent (sonnet)
**Scope**: tddy-agent-launch spawn paths, jail homes, daemon RPC services, web transport for terminal sessions, usage-dropping drivers, accounting.json writer, Codex/OpenCode specs

### Discovery dump: daemon-side per-conversation token accounting (read-only pass)

Repo root: `/Users/mantasi/Code/tddy-coder/.worktrees/pr-stack-accounting-tokens`. All paths below are relative to `packages/` unless they start with `/` or `docs/`.
`.codegraph/` does not exist, so I used grep, find and Read only.
HEAD is `bfccc2b6f`. The only untracked file is `docs/dev/1-WIP/2026-11-10-usage-accounting-whole-work-initial-discovery.md`, which is the parent's earlier dump. I read it and did not duplicate it. My findings are consistent with it and extend it.

---
## 1. Sequence (chronological)

1. `ls` repo root, `ls .codegraph` (absent), `ls packages`, `ls tddy-coding-agent-specs/**`. Purpose: layout and whether codegraph exists.
2. Read `tddy-coding-agent-specs/specs/claude.yaml` in full. Purpose: home, conversation, `transcript_usage`, headless block.
3. Read `specs/codex.yaml`, `specs/codex-acp.yaml` and `specs/opencode.yaml`. Purpose: Q6.
4. `git log`, `git status`. Purpose: baseline and the untracked WIP doc.
5. Grep `transcript_usage` across packages and docs. Purpose: consumers of the spec flag.
6. Read `docs/dev/1-WIP/2026-11-10-usage-accounting-whole-work-initial-discovery.md`. Purpose: avoid duplicating the parent's prior findings.
7. Grep `conversation` in `tddy-coding-agent-specs/src/spec.rs` and `src/spec/*.rs`. Read `src/slots/conversation.rs`. Purpose: `ConversationSlot`.
8. Grep `homes`, `HomeSection` and `JailHome` across packages. Read `tddy-sandbox-recipes/src/homes.rs`. Read `tddy-agent-launch/src/jail_launch.rs` (lines 1-330 and 330-520). Purpose: how the persistent home and the jail launch work.
9. Read `tddy-agent-launch/src/jail_session_files.rs` (`prepare_jail_dirs`) and `svc_start_sandboxed_claude_cli_session.rs` (`JailSession`, `JailDirs`). Purpose: where the session dir and egress dir come from.
10. Read `svc_start_sandboxed_cli_harness_session.rs` in full, `spec_launch.rs` (lines 1-394), and `harness_from_spec.rs` (200-640). Purpose: sandboxed start and `HostAgent` resolution.
11. Read `cli_agent_steps/conversation.rs`. Purpose: how `ConversationSlot` becomes a launch conversation.
12. Read `cli_harness_jail.rs` (`launch_jailed_cli_agent`). Purpose: the call that hands the persistent home to `launch_jailed`.
13. Read `svc_start_session_core.rs` (the dispatch) and `cli_branch_starts.rs`. Purpose: the CLI-harness path versus the workflow `spawn_tool_session` path.
14. Read `opencode_cli_spawn.rs` (lines 150-415) and `opencode_cli_spawn/harness_launch.rs`. Purpose: the host PTY launch path.
15. Grep `spawn_rendered_main`, `"HOME"`, `pty_user_env_overrides`. Read `tddy-daemon-kernel/src/user_paths.rs` and `privilege_drop.rs`. Purpose: HOME for host PTY sessions.
16. Read `tddy-daemon-sandbox/src/tool_jail.rs` (`lower_tool_jail`). Read `tddy-sandbox-recipes/src/tool_grants.rs` (`lower_home`). Read `tddy-actions/src/tool/home.rs` (`HomeSection`). Read `tddy-sandbox/src/runner_env.rs`. Read `specs/agent-env.yaml`. Purpose: jail HOME and env.
17. Grep `TDDY_TOOLS_ACCOUNTING_FILE` and `accounting.json` across packages. Read `tddy-discovery/src/subagent_runtime.rs` (560-760). Read `tddy-agent-backend/src/token_accounting.rs`, `backend/mod.rs` (100-185) and `backend/claude/transcript_usage.rs`. Purpose: who writes `accounting.json` and what shape.
18. `find` all `*.proto` files. Grep `unbundle_service_split` and `residual` (case-insensitive). Read `tddy-service/tests/unbundle_service_split.rs` (1-60, 140-200, 540-700) and `tests/service_coordinates.rs`. Purpose: Q2.
19. Grep `^service|^  rpc` across the session, activity, session_room, session_agents, tasks, terminal and remote protos. Read `proto/activity.proto` (1-100) and `proto/tddy/v1/remote.proto` (95-160). Purpose: Q2 and Q5.
20. Read `tddy-service/build.rs` (40-100). Purpose: what a new proto package costs.
21. Read `tddy-session-web/src/SessionInspectorDrawer.tsx` (40-135, 200-320), `useSessionUsage.ts`, `SessionUsageTab.tsx` and `SessionMainPane.tsx` (440-520). Grep `serverIdentity` and `room=`. Purpose: Q3.
22. Read `SessionsDrawerScreen.tsx` (60-160, 395-430), `useSessionsDrawerClients.ts`, `sessionPaneClients.ts` and `useSessionAgentRoster.ts`. Read `tddy-client-web/src/connections/{sessionAttachment,session,types}.ts`. Purpose: how the web reaches a session and a model for a per-session stream.
23. Read `tddy-agent-launch/src/session_coordinate_handlers.rs` (140-240) and `tddy-spawn/src/spawner.rs` (255-300, 835-870, 1030-1070). Read `tddy-daemon-livekit/src/session_reach.rs` and `tddy-session-lifecycle/src/connection_service/terminal_bridge_impl.rs`. Purpose: ConnectSession reply and server identity.
24. Read `tddy-daemon/src/runtime/mount_entries.rs`. Read `tddy-session-agents/src/service.rs` (790-960) and `svc_peer_routed_session_agents.rs`. Read `tddy-session-lifecycle/.../daemon_rpc_handler.rs` (1-120). Purpose: how services are mounted and a per-session snapshot stream pattern.
25. Read `tddy-agent-backend/src/backend/headless_driver/{stream_json,transcript,codex_jsonl}.rs`, `stream/codex.rs`, and `backend/acp_driver/{mapping,client}.rs`. Read `tddy-coding-subagent/src/session/subagent_session.rs` and `session.rs`. Purpose: Q4.
26. Grep `run_turn`, `subagent_sessions()`, `start_headless`, `CodingAgentSession::start`. Read `tddy-tools/src/server.rs` (1925-2000) and `tddy-session-agents/src/coding_agent.rs` (170-230). Purpose: which process runs the drivers.
27. Read `tddy-presenter/src/usage_watcher.rs` (55-215). Read `tddy-coder/src/run.rs` (1585-1600, 2090-2135). Purpose: the existing watcher.
28. Read `tddy-changeset/src/session_metadata.rs` (`SessionMetadata`). Grep `rollout`, `opencode.db`, `CODEX_HOME`, `XDG_DATA_HOME`. Purpose: what the daemon records per session, and where Codex and OpenCode write.

---
## 2. Inspected files (why, and the excerpts that informed findings)

### `tddy-coding-agent-specs/specs/claude.yaml`
```yaml
run: { transport: pty, argv: ["{binary}", {if: model...}, { if: resume, then: ["--resume","{session_id}"], else: ["--session-id","{session_id}"] }, ...] }
extensions.coding_agent:
  conversation: { kind: daemon_session_id }          # line 47
  transcript_usage: true                              # line 56
  headless: { argv: [... -p, --output-format, stream-json, --verbose ...], adapter: claude-stream-json, ... }
home: { kind: persistent, key: claude }               # jail section, near the end
files:
  - { kind: seed-once, from_host: .claude/.credentials.json, to: .claude/.credentials.json }
  - { kind: mirror, from_host: .local/bin/claude, to: .local/bin/claude }
  - { kind: overlay, env: CLAUDE_CODE_TMPDIR, value: "{tmp}" }   # and CLAUDE_TMPDIR
```
`claude-acp.yaml` also has `transcript_usage: true` (line 56) and `placements: [host]` (line 61).

### `tddy-coding-agent-specs/src/slots/conversation.rs`
```rust
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversationSlot {
    DaemonSessionId,                                   // bound as `session_id`
    Minted { adapter: String, mint_deadline_secs: Option<u64> },   // bound as `chat_id`
    Worktree,                                          // --continue
    ResumeToken,                                       // coding_agent.resume.adapter
}
```
Doc comment: a spec that declares none is `resume_token` if it has a `resume` slot, else `worktree` (`CodingAgentSpec::conversation()`, `src/spec.rs:295-299`).
- Only `claude.yaml:47` (`daemon_session_id`) and `cursor.yaml:54` (`minted`, adapter `cursor-create-chat`) declare one.
- `codex`, `codex-acp` and `opencode` declare none and have no `resume` slot, so they derive to `Worktree`.
- `warp` derives to `ResumeToken`.
- `src/spec.rs:249` holds the `transcript_usage: bool` field.

### `tddy-actions/src/tool/home.rs`
```rust
pub enum HomeSection {
    Persistent { key: String },   // <tddyhome>/homes/<key>, mounted rw
    Path { path: PathBuf },
    #[default] Session,           // lives and dies with the session
}
```

### `tddy-sandbox-recipes/src/homes.rs`
```rust
pub const HOMES_DIR: &str = "homes";
pub fn persistent_home(tddy_home: &Path, key: &str) -> Result<PathBuf, HomeError> {
    let home = tddy_home.join(HOMES_DIR).join(key);   // + migration of legacy <tddyhome>/sandbox-<key>-home
    ... std::fs::canonicalize(&home)
```

### `tddy-sandbox-recipes/src/tool_grants.rs` (`lower_home`)
- `Persistent` lowers to `persistent_home(paths.tddy_home, key)` plus a `MountSpec::read_write(home)`.
- `Session` lowers to `paths.session_home` with no mount.

### `tddy-daemon-sandbox/src/tool_jail.rs` (`lower_tool_jail`, lines ~74-135)
```rust
let session_home = scratch_dir.join("home");            // scratch_dir = <session_dir>/sandbox/.work
let home = lower_home(&tool_def.home, &paths)...;
let mut env = file_grants.env;
env.insert("HOME".to_string(), home.home.to_string_lossy().into_owned());
Ok(ToolJailGrants { home: home.home, env, mounts: home.mount..., ... })
```
`ToolJailGrants.home` is the agent's host-side HOME. It is the only place the final path exists, and it is not stored anywhere after the launch.

### `tddy-agent-launch/src/jail_launch.rs`
```rust
pub struct JailSpec<'a> { pub agent: &'a HostAgent, pub defs: &'a [ToolDef], pub host: &'a dyn ToolHost }   // :~57
pub struct JailedSession<'a> { session_type, jail: &'a JailSession, dirs: &'a JailDirs, session_env, env, managed, managed_codebase, route }
pub(crate) fn mcp_server_env(session_id, dirs, session_env) -> BTreeMap<..> {   // :95
   ... ("TDDY_TOOLS_ACCOUNTING_FILE", path(&dirs.egress_dir.join("accounting.json"))),   // :115-116
pub(crate) fn jail_home(def: &ToolDef, tddy_home, session_home) -> Result<PathBuf, Status>   // :~292
impl LaunchSessions { pub async fn launch_jailed(&self, spec: JailSpec, params: &TemplateParams, slots: &RenderedSlots,
                                                 session: JailedSession, role: &str) -> Result<u32, Status> {   // :~133
   let JailSession { session_id, project_id, session_dir, worktree_path, .. } = *session.jail;
   let grants = lower_tool_jail(&ToolJailInputs { tool: &spec.agent.tool, ..., tddy_home: &self.tddy_data_dir,
        host_home: &host_home()?, scratch_dir: &session.dirs.scratch_dir, ... })?;
   ...
   self.sandbox_manager.insert(session_id.to_string(), state).await; self.record_jailed_main(session_id).await;
```
- `exec_plan()` builds the agent env from `spec.run.env`, then the session env minus `TDDY_*`, then `grants.env` (HOME and overlays), then PATH.
- No `XDG_*`, `CODEX_HOME` or `OPENCODE_*` data-dir variables are set by the launcher.
- `jail_env` uses `tddy_sandbox::scratch_runner_env(&scratch_dir.join("home"), ...)`, which sets only HOME, TMPDIR, TERM and PATH (`tddy-sandbox/src/runner_env.rs`).

### `tddy-agent-launch/src/svc_start_sandboxed_claude_cli_session.rs`
```rust
pub struct JailSession<'a> { session_id: &'a str, project_id: &'a str, session_token: &'a str, session_dir: &'a Path, worktree_path: &'a Path }
pub struct JailDirs { sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir }
```
### `tddy-agent-launch/src/jail_session_files.rs` (`prepare_jail_dirs`)
```rust
let sandbox_root = session_dir.join("sandbox");  let egress_dir = session_dir.join("egress");
... let scratch_dir = sandbox_root.join(".work");  let context_dir = sandbox_root.join("context");
// comment: "scratch_home (jail $HOME) is the persistent daemon-wide claude home, resolved and mounted below"
```
### `write_jail_session_metadata` (same file, line 101)
It writes `.session.yaml` with `sandbox: Some(true)`, `model`, `pid`, `session_type` (via `starting_session_metadata`), `cursor_chat_id`, `hook_token` and the agent roster.

### `tddy-agent-launch/src/svc_start_sandboxed_cli_harness_session.rs`
```rust
let session_dir = start.sessions_base.join(SESSIONS_SUBDIR).join(session_id);
... let dirs = prepare_jail_dirs(&session_dir)?;
let jail = JailSession { session_id, project_id, session_token, session_dir: &session_dir, worktree_path: &worktree_path };
let conversation = start_conversation(ConversationStart { spec: &session_agent.agent.spec, binary: &..tool.binary, env_base, worktree, env: &[], session_id }).await?;
let params = harness_template_params(&conversation.bound(HarnessLaunch { model, prompt, permission_mode, ... }));
let pid = self.launch_jailed_cli_agent(CliJail { session_agent, jail: &jail, dirs: &dirs, params, route_args, env, session_env, route, wiring }).await?;
write_jail_session_metadata(&jail, JailRecord { session_type: kind.name, ..., chat_id: conversation.recorded_chat_id(), ... })?;
```
### `tddy-agent-launch/src/cli_harness_jail.rs` (`launch_jailed_cli_agent`, :247)
```rust
let CliSessionAgent { agent, agents, kind, .. } = launch.session_agent;
let home = if toolset.mcp_server.is_some() { Some(jail_home(tool_def, &self.tddy_data_dir, &launch.dirs.scratch_dir.join("home"))?) } else { None };
... self.launch_jailed(JailSpec { agent, defs: &defs, host: agents.host() }, &params, &slots, JailedSession { session_type: kind.name, jail, dirs, ... }, wiring.role)
```
Line 192 wires `mcp_server_env(launch.jail.session_id, launch.dirs, &launch.env)` into the `tddy-tools --mcp` registration.

### `tddy-agent-launch/src/spec_launch.rs`
```rust
pub struct HostAgent { pub spec: CodingAgentSpec, pub tool: ResolvedTool, pub deps: Vec<ResolvedTool>, pub env_base: AgentEnvBase }   // :147
impl HostCodingAgents { pub fn resolve(&self, id: &str, binary_override: Option<&Path>) -> Result<HostAgent, SpecLaunchError> }
pub async fn launch_host_pty(agent, params, slots, worktree, session: HostPtySession, on_exit) -> Result<Arc<PtyHandle>, ..>
```
`HostPtySession { manager, session_id, session_type, model, env, os_user }`. The host spawn is `session.manager.spawn_rendered_main(session_id, session_type, worktree, model, argv, env, os_user, on_exit)`.

### `tddy-agent-launch/src/harness_from_spec.rs` (:256)
```rust
pub(crate) struct CliSessionAgent { pub kind: CliSessionType, pub agent: HostAgent, pub agents: Arc<HostCodingAgents> }
pub(crate) fn resolve_session_agent(agents: &Arc<HostCodingAgents>, kind: CliSessionType) -> Result<CliSessionAgent, Status> {
    agents.resolve(kind.spec_id, None)...
```
### `tddy-coding-agent-specs/src/registry.rs:18,126`
```rust
pub const CLI_SESSION_TYPE_SUFFIX: &str = "-cli";
pub fn for_session_type(&self, session_type: &str) -> Option<&CodingAgentSpec> {
    let id = session_type.strip_suffix(CLI_SESSION_TYPE_SUFFIX)?; self.get(id).filter(|spec| spec.run.transport == Transport::Pty) }
```
### `tddy-agent-launch/src/svc_start_session_core.rs`
Dispatch in `start_here_body` (the workspace branch comes first, then the CLI-agent branch):
```rust
if let Some(kind) = cli_launcher(Some(&*self.coding_agents), req.session_type.trim())? {
    return self.start_cli_harness(kind, &req, progress, os_user).await; }
let livekit = spawner::livekit_creds_from_config(&self.config)...   // else: workflow ("tool") session
let result = self.spawn_tool_session(req, progress, os_user, agent_def, livekit, &project).await?;
```
### `tddy-agent-launch/src/cli_branch_starts.rs`
- `cli_start_prelude` sets `sessions_base = user_paths::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))` and `session_id = Uuid::now_v7()`.
- `start_cli_harness` branches on `req.sandbox`:
  - `start_sandboxed_cli_harness_from_request` is the jail path.
  - `start_cli_harness_from_request` calls `opencode_cli_spawn::start_cli_harness_session`, the host PTY path.

### `tddy-agent-launch/src/opencode_cli_spawn.rs` (the host PTY start)
- `session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id)`.
- It calls `start_conversation(...)`, then `launch(cli_manager, HarnessSession::new(...), HarnessSpecLaunch{agent, params, slots, kind, model, env, on_exit}, HarnessRun::start(run_in_tmux))`.
- Then it writes `SessionMetadata`, then `cli_manager.expose_terminal_to_livekit(session_id, claude_cli_participant_metadata(...))`.
- It passes `home: None` in `HostLaunchSlots`. The host agent HOME is not set by the spec.
- No `TDDY_TOOLS_ACCOUNTING_FILE` is set on host PTY sessions. A grep across `tddy-agent-launch/src` and `tddy-cli-sessions/src` finds it only at `jail_launch.rs:115`.

### `tddy-daemon-kernel/src/privilege_drop.rs`
```rust
pub fn pty_user_env_overrides(home_dir: &Path, path_extra: Option<&str>) -> Vec<(String,String)> { vec![("HOME", home_dir...), ("PATH", merge_spawn_child_path(path_extra))] }
```
`home_dir_for_user(os_user)` in `user_paths.rs:17` resolves the passwd home via `getpwnam_r`. `tddy-cli-sessions/.../launch.rs:48-60` calls `spawn_tool_on(... PtyEnvBase::Cleared, os_user, ...)`.

### `tddy-agent-launch/src/cli_agent_steps/conversation.rs`
```rust
pub(crate) enum LaunchConversation { SessionId(String), Minted(String), Worktree, ResumeToken(Option<String>) }
start_conversation(...): ConversationSlot::DaemonSessionId => Ok(LaunchConversation::SessionId(session_id.to_string())), Worktree => Worktree, ResumeToken => ResumeToken(None), Minted{adapter,..} => LaunchAdapters::builtin().conversation_id(adapter)...mint(...)
resumed_conversation(spec, meta): DaemonSessionId => SessionId(meta.session_id.clone()); Minted => meta.cursor_chat_id; ResumeToken => meta.harness_resume_token
```
So for Claude, the conversation id equals the daemon session id, and the transcript file is `<session_id>.jsonl`.

### `tddy-agent-backend/src/token_accounting.rs` (Q5)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage { pub input_tokens: u64, pub output_tokens: u64 }          // :14
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {                                                   // :41
    pub agent: String, pub id: String, pub model: String,
    #[serde(rename = "inputTokens")]  pub input_tokens: u64,
    #[serde(rename = "outputTokens")] pub output_tokens: u64,
    #[serde(rename = "totalTokens")]  pub total_tokens: u64,
    pub turns: u32,
}
```
`format_token_summary` renders the table. The docs call the file shape the wire shape shared by the `subagent_list` MCP tool, the accounting file and the summary renderer.

### `tddy-discovery/src/subagent_runtime.rs:609-620` (writer)
```rust
/// Overwrite the host-visible accounting file (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner into the session egress dir) ...
pub fn write_accounting_file(conversations: &SubagentConversations) {
    let Some(path) = env_non_empty("TDDY_TOOLS_ACCOUNTING_FILE") else { return; };
    let payload = serde_json::json!({ "conversations": conversation_records(conversations) });
    if let Ok(text) = serde_json::to_string_pretty(&payload) { let _ = std::fs::write(&path, text); }
}
```
Callers: `subagent_runtime.rs:702` and `:750`, and `tddy-tools/src/server.rs:2206` and `:2382`. It is a whole-file overwrite after each turn. `conversation_records` includes retired conversations.

### `tddy-agent-backend/src/backend/mod.rs:124` (`gather_session_usage`)
```rust
pub fn gather_session_usage(session_dir, session_id, claude_home, fallback_model, include_main_agent: bool) -> Vec<ConversationRecord> {
    ... if include_main_agent { push(read_claude_transcript_usage(claude_home, session_id, fallback_model)); extend(read_claude_subagent_usages(...)); }
    let accounting_path = session_dir.join("egress").join("accounting.json");   // :153
    ... struct AccountingFile { #[serde(default)] conversations: Vec<ConversationRecord> }
```
### `tddy-agent-backend/src/backend/claude/transcript_usage.rs`
- `read_claude_transcript_usage(claude_home, session_id, fallback_model)` scans `<claude_home>/.claude/projects/*/<session_id>.jsonl`. It scans every project dir, so it does not need the cwd encoding.
- `read_claude_subagent_usages` reads `projects/*/<session_id>/subagents/agent-*.jsonl` plus `.meta.json` `agentType`.
- `sum_assistant_usage` sums `message.usage.input_tokens` and `output_tokens` per `type=="assistant"` line, with no dedupe by `message.id`. It ignores the `cache_*` counters.

### Protos (Q2 and Q5)
`tddy-service/proto/tddy/v1/remote.proto:7-10`, `:112`, `:119-135`:
```proto
service TddyRemote { rpc Stream(stream ClientMessage) returns (stream ServerMessage); rpc GetSession(...); rpc ListSessions(...); }
message ServerMessage { oneof event { ... TokenUsageUpdated token_usage_updated = 13; } }
// Per-session token usage snapshot: the full cumulative per-conversation breakdown (never a delta). Emitted on connect and whenever a conversation's usage changes; the client replaces its held snapshot on each event.
message TokenUsageUpdated { repeated ConversationRecord conversations = 1; }
message ConversationRecord { string agent=1; string id=2; string model=3; uint64 input_tokens=4; uint64 output_tokens=5; uint64 total_tokens=6; uint32 turns=7; }
```
The proto has no `ConversationUsage` message. That name exists only on the TS side (`tddy-session-web/src/sessionUsage.ts`, mapped in `useSessionUsage.ts`). The wire message is `ConversationRecord`. The proto has no cache or cost fields.

### `tddy-service/tests/unbundle_service_split.rs`
- `:570` `const RESIDUAL_METHODS: [&str; 19]` is a closed-world list covering exactly `SessionService` (session.proto), `ProjectService` (project.proto), `DemoVmService` (demo_vm.proto) and `LocalTokenService` (local_token.proto). It is enforced by `connection_service_ends_at_exactly_the_residual` (`:631`).
  - `session.proto` `SessionService` methods: ListSessions, StartSession, StreamStartSession, ConnectSession, ResumeSession, EnsureProjectHomeSession, SignalSession, DeleteSession, GetWorktreeSnapshot.
  - The doc comment says a new method on one of these four services joins the list deliberately, in the change that adds it.
- `:469` `ACTIVITY_METHODS: [&str; 8]` is a "declares" check (`block.contains("rpc X(")`), not closed. Adding to `ActivityService` does not break it.
- `:507` `the_sandbox_relay_allowlist_names_the_new_service` is only relevant if an in-jail relay is added (`tddy-sandbox-runner/src/runner.rs` `IN_JAIL_RELAYABLE`).
- `:~549` `the_shared_types_file_holds_only_the_five_types_that_genuinely_cross`: `types.proto` must hold exactly 5 declarations. Do not add shared types there.
- `tests/service_coordinates.rs` only pins the `package.Service` string for `session_files.proto`. It is not a closed list.

### `tddy-service/build.rs:53-85`
`tonic_service_module(package)` and `rust_module(package)` `panic!("no ... known for proto package {other}")` for an unknown package. A new `.proto` package therefore needs entries there. Adding an rpc to an existing proto such as `activity.proto` does not.
Generated TS lives in `tddy-client-web/src/gen/*_pb.ts` (e.g. `activity_pb.ts`, `tddy/v1/remote_pb.ts`), generated by `../../scripts/generated-code.sh write packages/tddy-client-web` (`package.json` "generate").

### `tddy-daemon/src/runtime/mount_entries.rs`
`mount_session_entries` pushes `connection_arc.{terminal_session_entry, session_files_entry, session_agents_entry, activity_entry, session_lifecycle_entry}()` onto `self.identity.rpc_entries`. These entries are served over HTTP `/rpc` and over the LiveKit common room (see the `HostService` comment). `ActivityService` is mounted via `activity_entry()` and implemented in `tddy-session-activity`.

### `tddy-service/proto/activity.proto`
`ActivityService` has these RPCs:
- `ReportSessionStatus`
- `StreamSessionActivity(StreamSessionActivityRequest) returns (stream AgentActivityRecord)`, request `{session_token, session_id, daemon_instance_id, mode}`. Replays then streams live and is served over HTTP for any live or dormant session.
- `ReportAgentActivity`
- `StreamSessionNotifications` (daemon-level)
- `StreamAgentActivityDelta`
- `StreamAcpReplay`
- `GetAcpToolCallDetail`
- `GetAcpReplayPage`
- `ReportOperatorNotification`
- `AwaitQuestionAnswer`
- `IsSessionTerminalServed`

### `session_agents.proto`: `SessionAgentService.StreamSessionAgents(StreamSessionAgentsRequest{session_token, session_id, daemon_instance_id}) returns (stream SessionAgentRoster)`
This is the closest existing model for a per-session snapshot stream.
- Server: `tddy-session-agents/src/service.rs:808`. It resolves `session_dir(token, session_id)`, subscribes to a broadcast, sends a snapshot first, then re-sends on every change and on a keepalive timer.
- Peer routing: `tddy-session-lifecycle/.../svc_peer_routed_session_agents.rs:150`, via `connection.stream_served_by_peer(SERVICE, method, &req.daemon_instance_id, req)`.
- Web: `tddy-session-web/src/useSessionAgentRoster.ts` uses `client.streamSessionAgents({sessionToken, sessionId, daemonInstanceId}, {signal})` and replaces state with each snapshot.

### Web (Q3)
`tddy-session-web/src/useSessionUsage.ts:57`:
```ts
const roomConnected = useRoomStatus(room) === "connected";
const client = useRoomClient(roomConnected ? room : null, serverIdentity, TddyRemote);
... for await (const serverMessage of client.stream(queue)) { if (serverMessage.event.case === "tokenUsageUpdated") { setUsage(...conversations.map(c => ({agent,id,model,inputTokens,outputTokens,totalTokens,turns}))) } }
```
`SessionInspectorDrawer.tsx:128-129,280`: props `room = null, serverIdentity = "server"` flow to `<SessionUsageTab room={room} serverIdentity={serverIdentity} />`.
`SessionMainPane.tsx:484-485`:
```tsx
room={room}
serverIdentity={attachmentHint?.serverIdentity}
```
`SessionsDrawerScreen.tsx:79`: `const room = useHostPresence(selectedInstanceId);`. This is the common room, passed on at `:422`. `attachmentHint` comes from `useSessionAttachment()` (`:123`).
`tddy-client-web/src/connections/sessionAttachment.ts:46-60`: `attachmentHintFromReply` leaves `serverIdentity` unset when `livekitRoom` is empty, and otherwise copies `livekitServerIdentity`.
Daemon side, `tddy-agent-launch/src/session_coordinate_handlers.rs:~178-225` (`connect_session_at_session_coordinate`):
```rust
if self.coding_agents.agents().for_session_type(session_type).is_some() || session_type == "workspace" {
    return Ok(Response::new(ConnectSessionResponse { livekit_room: String::new(), livekit_url: String::new(), livekit_server_identity: String::new() })); }
...
let livekit_server_identity = spawner::livekit_server_identity_for_session(instance.as_deref(), &req.session_id);  // workflow sessions
```
`tddy-spawn/src/spawner.rs:269`: identity is `daemon-{instance}-{session_id}`, or `daemon-{session_id}` with no instance.
`terminal_bridge_impl.rs:30-60` (`expose_session_terminal`) exposes a CLI terminal in `resolve_livekit_room_name(common_room, session_id)` under that same identity.

### Drivers (Q4)
`tddy-agent-backend/src/backend/headless_driver/stream_json.rs`:
```rust
struct Event { kind, session_id: Option<String>, result: Option<String>, is_error: bool, parent_tool_use_id, message: Option<Message> }   // :16-27
struct ResultEvent { text: Option<String>, is_error: bool }   // :70
 ...
 "result" => { self.note_session(event.session_id); self.result = Some(ResultEvent { text: event.result, is_error: event.is_error }); }   // :140-145
```
`Event` declares no `usage`, `modelUsage`, `total_cost_usd`, `num_turns` or `duration_*`, so serde silently ignores them. `finish()` (`:168`) returns `Finished { session_id, result }` and the result is `AcpTurnEnd { stop_reason, text }` (`transcript.rs`). Neither carries usage.
`tddy-agent-backend/src/stream/codex.rs:65-67`:
```rust
_ => { log::debug!("[tddy-codex] jsonl event type: {}", ty); }
```
Every other type is dropped, including Codex's `turn.completed`. I did not see a Codex sample here, so the usage field name is a Codex JSONL detail to verify. `CodexStreamResult` is only `{result_text, session_id}`. `headless_driver/codex_jsonl.rs` `CodexTurn::finish` calls `parse_codex_jsonl_output` over the collected lines and returns no usage.
`tddy-agent-backend/src/backend/acp_driver/mapping.rs:16-17,43`:
```rust
/// ... Updates the driver does not report — available commands, mode, configuration and usage changes — map to `None`.
pub(super) fn acp_update(update: acp::SessionUpdate) -> Option<AcpUpdate> { match update { ... _ => None, } }
```
`acp_driver/client.rs:98`: `if let Some(update) = mapping::acp_update(notification.update) { self.turns.record(...) }`. The ACP usage-update variant is dropped at the `_ => None` arm.
`tddy-coding-subagent/src/session/subagent_session.rs:108-111`:
```rust
/// ACP reports no token usage, so there is nothing to total.
fn cumulative_usage(&self) -> TokenUsage { TokenUsage::default() }
fn context_tokens(&self) -> u64 { 0 }
```
Also `Settled::Paused` builds `PromptOutcome::new(.., TokenUsage::default())` at `:~82`.

Where the drivers run:
- `tddy-session-agents/src/coding_agent.rs:191,223,226` (`start_headless_session` and `start_acp_session`) is called from `SessionAgentService.open_agent_conversation` (`tddy-session-agents/src/service.rs:~880-940`, `ports.sessions.open_local`). Coding subagents (headless `claude -p` through `claude-stream-json`, codex-jsonl, oz-ndjson, and ACP) therefore run in the daemon process, or behind a supervisor-started stream (`start_over`).
- An in-jail `tddy-tools --mcp` that holds no def for the agent opens the conversation remotely with `open_remote_agent_session` (`tddy-tools/src/server.rs:1953-1976`). It records a `SubagentConversation` whose usage comes from the remote proxy session. Those rows are zero.
- The daemon-side `AgentConversation::Local` (`service.rs:73`) is not written to `accounting.json`.
- `tddy-agent-backend` `SpecHeadlessBackend` (`backend/spec_headless.rs`, `claude.rs`, `codex.rs`) is the workflow backend used by `tddy-coder --agent ...`. It runs in the tddy-coder process, not in the daemon.

### `tddy-presenter/src/usage_watcher.rs` and `tddy-coder/src/run.rs`
- `run.rs:1590` `resolve_agent_home_dir()` returns `$HOME` or `$USERPROFILE`.
- `run.rs:~2108-2125` spawns `spawn_session_usage_watcher(SessionUsageWatchConfig{ session_dir: session_artifact_dir, session_id: args.session_id, claude_home: resolve_agent_home_dir(), agent: agent_str, model, poll_interval })`.
- `usage_watcher.rs` sets `include_main_agent: config.agent == "claude"` (a hard-coded agent name).
- It runs `gather_session_usage` on `spawn_blocking` each tick and broadcasts a full snapshot only when it changed (`SessionUsageEmitter::emit_if_changed`).

### `tddy-changeset/src/session_metadata.rs`
`SessionMetadata` has `session_id`, `project_id`, `session_type`, `model`, `sandbox: Option<bool>`, `agent`, `recipe`, `cursor_chat_id`, `harness_resume_token`, `pid`, `livekit_room` and others. It has no agent-home field, no transcript path and no conversation id. The jail start writes `sandbox: Some(true)`. The host harness start writes no `sandbox` (it is `None`).

---
## 3. Grep / glob log (patterns, scope, notable hits)

| Pattern / glob | Scope | Notable hits |
|---|---|---|
| `.codegraph` | repo root | absent |
| `transcript_usage` | packages, docs (`*.rs`, `*.yaml`, `*.md`) | `sandbox-app/src/main.rs:578`, `coding-agent-specs/src/spec.rs:249`, `specs/claude.yaml:56`, `specs/claude-acp.yaml:56`, `slots/mod.rs:285`, `agent-backend/src/backend/claude/transcript_usage.rs` |
| `conversation:` | `specs/*.yaml` | `claude.yaml:47` (`daemon_session_id`), `cursor.yaml:54` (`minted`) only |
| `homes\b` and `HomeSection` (rs) | packages | `sandbox-recipes/src/homes.rs`, `tool_grants.rs:232`, `agent-launch/src/jail_launch.rs:35,290`, `sandbox-app/src/main.rs:287` (macOS standalone) |
| `TDDY_TOOLS_ACCOUNTING_FILE\|accounting.json` | packages (rs) | writer `tddy-discovery/src/subagent_runtime.rs:613`; env set `agent-launch/src/jail_launch.rs:115` (daemon jail), `sandbox-app/src/spawn.rs:623` and `host_agent.rs:116` (standalone macOS app); reader `agent-backend/src/backend/mod.rs:153`; `presenter/src/usage_watcher.rs` |
| `CODEX_HOME\|XDG_DATA_HOME\|XDG_CONFIG_HOME` | packages (rs, yaml, md) | none set by any launcher. Only tests and `agent_env_tests.rs` (XDG_* passthrough of the host env via `agent-env.yaml` `XDG_*`) |
| `rollout\|opencode.db\|.local/share/opencode` | packages, docs | none. Nothing in the repo reads or names these paths |
| `unbundle_service_split\|residual` (-il) | packages, docs | `tddy-service/tests/unbundle_service_split.rs` (the real closed-world test), `tests/service_coordinates.rs`, a few changesets/docs |
| `*.proto` find | packages | 50+ protos. Relevant: `tddy-service/proto/{session,activity,session_agents,session_room,terminal,tasks}.proto`, `tddy/v1/remote.proto` |
| `^service \|^  rpc ` | session/activity/session_room/session_agents/tasks/terminal/remote protos | see Finding 2 |
| `launch_host_pty\|plan_host_pty\|fn start_*` | packages | `agent-launch/src/spec_launch.rs:248,277`, `opencode_cli_spawn/harness_launch.rs:114,133` |
| `livekit_server_identity` | packages (rs) | `session_coordinate_handlers.rs:200-225`, `tddy-spawn/src/spawner.rs:269`, `terminal_bridge_impl.rs:40`, `tddy-daemon-livekit/src/session_reach.rs:168` |
| `serverIdentity` | tddy-session-web/src, tddy-client-web/src | `SessionMainPane.tsx:485`, `SessionInspectorDrawer.tsx:91,129,280`, `useSessionUsage.ts`, `connections/sessionAttachment.ts:53-58`, `connections/session.ts:42` |
| `run_turn\|subagent_sessions()\|start_headless\|CodingAgentSession::start` | tddy-tools, tddy-session-agents, tddy-coding-subagent | `tddy-session-agents/src/coding_agent.rs:191,223,226`; `tddy-tools/src/server.rs:1954,1991,2231` |
| `cumulative_usage` | packages | `coding-subagent/src/session/subagent_session.rs:109`, `tddy-discovery/src/subagent_runtime.rs:101,687` |
| `mcp_server_env(` | packages | `agent-launch/src/jail_launch.rs:95`, `cli_harness_jail.rs:192` |

---
## 4. Findings (numbered to match the 6 questions)

### 1. Session spawn, agent spec, HOME, session dir, conversation id

There are three spawn shapes. Which one runs is decided in `tddy-agent-launch/src/svc_start_session_core.rs` `start_here_body`:

(a) Terminal/PTY session on the host (not sandboxed): `cli_launcher(...)` matches `<id>-cli` → `LaunchSessions::start_cli_harness` (`cli_branch_starts.rs`) → `start_cli_harness_from_request` → `opencode_cli_spawn::start_cli_harness_session` (`opencode_cli_spawn.rs`).
- Spec resolution: `resolve_session_agent(&self.coding_agents, kind)` → `HostCodingAgents::resolve(spec_id, None)` → `HostAgent { spec, tool, deps, env_base }`.
- `HostAgent` is built from `tddy_coding_agent_specs::CodingAgentSpec::of(def)` over `ToolRegistry`. `CodingAgentRegistry::for_session_type` strips `-cli` and requires `Transport::Pty`.
- Session dir: `<sessions_base>/sessions/<session_id>`. `sessions_base` is `sessions_base_for_user(os_user, Some(&self.tddy_data_dir))`, which is `tddy_data_dir` when configured. `SESSIONS_SUBDIR = "sessions"` (`tddy-session-store/src/output/writer.rs:100`). `unified_session_dir_path` is at `tddy-changeset/src/session_lifecycle.rs:45`.
- Agent HOME: not controlled by the spec. `PtyEnvBase::Cleared` plus `pty_user_env_overrides` sets HOME to `home_dir_for_user(os_user)`, the passwd home (not `<tddyhome>/homes/<key>`). The `home:` section of the YAML is ignored on the host path (`HostLaunchSlots { home: None }`).
- Host transcripts for Claude are at `~<os_user>/.claude/projects/<encoded-cwd>/<session_id>.jsonl` (`claude_home` = passwd home). A tmux run (`run_in_tmux`) uses the same HOME.
- No `egress/accounting.json` is ever written for these sessions. `TDDY_TOOLS_ACCOUNTING_FILE` is only set by the jail launcher.
- Conversation id: `start_conversation` returns `LaunchConversation::SessionId(session_id)` for `daemon_session_id`. It is bound as `session_id` into `claude --session-id <id>` / `--resume <id>`. The daemon session id and the transcript file stem are identical for Claude.

(b) Terminal session in a jail (`req.sandbox`): `start_sandboxed_cli_harness_from_request` → `start_sandboxed_cli_harness_session` (`svc_start_sandboxed_cli_harness_session.rs`) → `launch_jailed_cli_agent` (`cli_harness_jail.rs:247`) → `LaunchSessions::launch_jailed` (`jail_launch.rs`).
- Session dir: the same `<sessions_base>/sessions/<id>`. `prepare_jail_dirs` creates `<session_dir>/sandbox`, `/egress` (the host-visible dir, canonicalized), `/sandbox/.work/{home,tmp}` and `/sandbox/context`.
- Jail HOME comes from `lower_tool_jail` → `lower_home(&tool_def.home, ...)`:
  - `home: persistent, key: claude` → `<self.tddy_data_dir>/homes/claude` (canonical, mounted rw, shared by all sessions, so it holds the transcripts of all Claude jailed sessions).
  - `home: session` (the `HomeSection` default, used by opencode) → `<session_dir>/sandbox/.work/home`.
  - `home: path` → the given absolute path.
- Claude's host-side transcript path is `<tddy_data_dir>/homes/claude/.claude/projects/<encoded-cwd>/<session_id>.jsonl`, plus `.../<session_id>/subagents/agent-*.jsonl`. The agent's cwd in the jail is `context_dir` (`<session_dir>/sandbox/context`). `read_claude_transcript_usage` takes `claude_home` = `<tddy_data_dir>/homes/claude` and scans every project dir by session id, so it needs no cwd encoding.
- `accounting.json` is `<session_dir>/egress/accounting.json`. It is written by the in-jail `tddy-tools --mcp` server, whose env block (`mcp_server_env`, `jail_launch.rs:95-125`) names the egress path.
- Metadata: `write_jail_session_metadata` records `sandbox: Some(true)`, `session_type`, `model`, `pid`.
- Struct with all of session id, session dir, resolved spec and agent home: no single struct has all four.
  - Inside `LaunchSessions::launch_jailed(spec: JailSpec, params, slots, session: JailedSession, role)`: `session.jail: &JailSession` carries `session_id`, `session_dir`, `worktree_path` and `project_id`; `spec.agent: &HostAgent` carries the resolved spec (`.spec: CodingAgentSpec`, `.tool`, `.deps`); `grants = lower_tool_jail(...)` (a `ToolJailGrants`) carries `.home`, the agent's HOME as a host path.
  - `CliSessionAgent { kind, agent: HostAgent, agents }` pairs the session type and `HostAgent`.
  - The nearest hook point is the end of `launch_jailed` (`jail_launch.rs`, just after `self.record_jailed_main(session_id).await;`). Everything is in scope there, and the home is `grants.home`.
  - Nothing persists the home. `SessionMetadata` has no home or transcript field, so for resume or restart the home must be recomputed from the spec (`jail_home(def, tddy_data_dir, scratch_dir/home)`, `jail_launch.rs:~292`).
- Resume of jailed sessions goes through `svc_resume_sandboxed_cli_harness_session.rs`, and relaunch through `jail_relaunch.rs` / `relaunch_jail_dirs.rs`. Both recompute `JailDirs`, with the same `session_dir/egress` and `session_dir/sandbox` layout.

(c) Workflow ("tool") session: not a CLI harness, so `cli_launcher` returns None and `start_here_body` falls through to `spawn_tool_session` (`tool_session_spawn.rs`) → `spawn_tddy_coder` → `tddy-spawn/src/spawner.rs`.
- This spawns a `tddy-coder --daemon` child as the OS user with `.env("HOME", &home_dir)` (`spawner.rs:1054`). The home is the passwd home, `target.home`.
- It joins LiveKit as `daemon-{instance}-{session}` in `livekit_room`. When `livekit.common_room` is set this is the common room (`resolve_livekit_room_name`, `spawner.rs:253`).
- tddy-coder runs the usage watcher (`run.rs:~2108`).
- The session dir is `unified_session_dir_path(sessions_base, session_id)`. The workflow agent's own HOME is the child's HOME (passwd home), so `claude_home` = `$HOME` (`run.rs:1590`).
- The daemon itself runs nothing for these sessions today.

Workflow headless agents (the tddy-coder `--agent claude` backend) do not use `home: persistent`. That applies only to the jail.

### 2. RPC services and where `SessionUsage` fits

- The daemon exposes every `ServiceEntry` in `rpc_entries` on HTTP `/rpc` (ConnectRPC) and on the LiveKit common-room RPC participant. The web reaches them with `HostConnection.clientFor(Service)` / `useDaemonClient(Service)`.
- The web's clients, from `tddy-session-web/src/useSessionsDrawerClients.ts`, are: SessionService, ProjectService, WorktreeService, TerminalSessionService, SessionFilesService, SessionAgentService, ActivityService, CatalogService, AccountsService, ExecToolService, PrStackService, TaskService.
- Per-session server streams the web already subscribes to on the daemon:
  - `ActivityService.StreamSessionActivity` (activity tab; snapshot then live, any session live or dormant).
  - `SessionAgentService.StreamSessionAgents` (roster; snapshot then changes, keepalive, peer-routed by `daemon_instance_id`).
  - `ActivityService.StreamSessionNotifications` (daemon-wide, not per-session).
  - `ActivityService.StreamAcpReplay`.
  - `StreamAgentActivityDelta`.
  - `TaskService.WatchTask` and `WatchTaskList`.
- Natural host for a `SessionUsage` stream: `ActivityService` in `tddy-service/proto/activity.proto`. It is session-scoped and already holds the per-session observation streams. It is served by `tddy-session-activity`, mounted via `connection_arc.activity_entry()`, and its `ACTIVITY_METHODS` check is a subset check, not closed. Alternatively `SessionAgentService.StreamSessionAgents` is the best behavioural template: request `{session_token, session_id, daemon_instance_id}`, whole snapshots, keepalive.
- Closed-world registry: `RESIDUAL_METHODS` in `tddy-service/tests/unbundle_service_split.rs:570` is closed only for `SessionService`, `ProjectService`, `DemoVmService` and `LocalTokenService`. Adding the stream to `SessionService` (session.proto) would require editing that list and the count (`[&str; 19]`). Adding it to `ActivityService` or `SessionAgentService` does not.
- A brand-new proto package would need `tonic_service_module` and `rust_module` entries in `tddy-service/build.rs`, otherwise the build panics. The `types.proto` count test (5 declarations) forbids shared types there. TS gen under `tddy-client-web/src/gen` must be regenerated with `scripts/generated-code.sh write packages/tddy-client-web`.
- The in-jail relay allowlist (`tddy-sandbox-runner/src/runner.rs`) only matters if an in-jail agent should call it, which is not the case for web usage.
- `TddyRemote` (remote.proto) is served only by `tddy-coder`'s LiveKit participant, so a terminal session has nothing serving it.

### 3. How the web Inspector reaches a terminal session

- `SessionUsageTab` → `useSessionUsage(room, serverIdentity)` → `useRoomClient(roomConnected ? room : null, serverIdentity, TddyRemote)`.
  - `room` is `useHostPresence(selectedInstanceId)`, the common LiveKit room (null if not joined or LiveKit is off).
  - `serverIdentity` is `attachmentHint?.serverIdentity`, which defaults to the literal `"server"` in `SessionInspectorDrawer.tsx:129`.
- For workflow sessions, `ConnectSession` returns room, url and identity `daemon-{[instance-]}{session_id}` (the tddy-coder participant), so the hook reaches `TddyRemote.Stream` on tddy-coder. For terminal (CLI) sessions and `workspace`, `ConnectSession` returns all empty strings (`session_coordinate_handlers.rs`). The hint has no room or identity, `serverIdentity` falls back to `"server"`, and there is no `TddyRemote` server in the common room under that identity. The Usage tab therefore cannot work for terminal sessions today.
- The terminal PTY itself is exposed in the common room under `daemon-{instance}-{session}` (`terminal_bridge_impl.rs`), but it serves the terminal service, not `TddyRemote`. All other inspector tabs reach the daemon with `client`, `activityClient`, `worktreeClient` and so on from `useDaemonClient(...)`, which is `/rpc` or LiveKit RPC (no LiveKit needed), routed to the owning host by `sessionPaneClients`.
- So a daemon-hosted usage stream would use `activityClient` (or a new client) the same way the Activity tab does, with no dependency on the room or identity.

### 4. Drivers that drop usage

| Driver | file:line | Data arriving | Where dropped | Process |
|---|---|---|---|---|
| Claude and Cursor stream-json | `tddy-agent-backend/src/backend/headless_driver/stream_json.rs:16-27,140-145` | `result` event (also carries `usage`, `modelUsage`, `total_cost_usd` per the brief; I did not see a sample line in the repo) | `Event` has no such fields; `"result"` stores only `{text,is_error}`; `finish()` returns `Finished{session_id,result:AcpTurnEnd{stop_reason,text}}` | daemon (coding subagent via `coding_agent.rs`); also tddy-coder (workflow `SpecHeadlessBackend`) |
| Codex jsonl | `tddy-agent-backend/src/stream/codex.rs:65-67` (`_ => debug!("[tddy-codex] jsonl event type: {}")`); `headless_driver/codex_jsonl.rs` `finish` | non-`item.completed` events including the turn-completion event with usage | logged at debug and discarded; `CodexStreamResult` has only `result_text`, `session_id` | same two processes |
| ACP | `tddy-agent-backend/src/backend/acp_driver/mapping.rs:16-17,43` (`_ => None`); `client.rs:98` | `session/update` incl. the usage-update variant | `acp_update` returns `None`, so `turns.record` is never called | daemon (subagent), tddy-coder (acp backends) |
| Coding subagent session | `tddy-coding-subagent/src/session/subagent_session.rs:108-111` | n/a | `cumulative_usage()` returns `TokenUsage::default()`, `context_tokens()` returns 0 | daemon |

- Consequence: `conversation_record` in `tddy-discovery/src/subagent_runtime.rs` reads `session.cumulative_usage()` after each turn, so these coding subagents report 0.
- Process detail: the daemon runs coding subagents through `SessionAgentService.open_agent_conversation` (`tddy-session-agents/src/service.rs`, `coding_agent.rs:191-226`).
  - In-jail `tddy-tools` forwards the open (`tddy-tools/src/server.rs:1953-1976`) and writes a proxy `SubagentConversation` into `accounting.json` with zero usage.
  - The daemon-side `AgentConversation::Local` rows are never written to `accounting.json`, because the daemon does not set `TDDY_TOOLS_ACCOUNTING_FILE`.
- `tddy-sandbox` / the jail runs only the agent CLI (`claude`, `opencode`) and the `tddy-tools --mcp` server. `tddy-sandbox-runner` runs inside the jail and holds no driver.

### 5. Who writes `accounting.json`, and shapes

- Writer: `tddy_discovery::subagent_runtime::write_accounting_file` (`subagent_runtime.rs:612`), run in the `tddy-tools --mcp` process. This is the in-jail MCP server for jailed sessions, and a host process for standalone `tddy-sandbox-app` sessions. It is called from `run_turn` after each turn, and from `tddy-tools/src/server.rs:2206,2382`.
- Path: `$TDDY_TOOLS_ACCOUNTING_FILE`, set to `<session_dir>/egress/accounting.json` by `mcp_server_env` (`jail_launch.rs:95-125`) for daemon jails. The standalone `tddy-sandbox-app` sets it at `spawn.rs:623` and `host_agent.rs:116`.
- Host PTY sessions started by the daemon never get this variable, so no `accounting.json` appears.
- Shape: `{"conversations": [ConversationRecord, ...]}`, pretty-printed, whole-file overwrite. The record is the Rust struct quoted above (camelCase `inputTokens`, `outputTokens`, `totalTokens`; `agent`, `id`, `model`, `turns`).
- Readers: `gather_session_usage` (`backend/mod.rs:124`), whose only two consumers are the presenter's `usage_watcher` (tddy-coder) and `print_token_summary` (`tddy-sandbox-app`, macOS).
- Proto: `TokenUsageUpdated { repeated ConversationRecord conversations = 1; }` and `ConversationRecord { agent, id, model, input_tokens, output_tokens, total_tokens, turns }` in `remote.proto:112,119-135`. The proto has no `ConversationUsage`; that is the TS type name only.
- Neither the Rust record nor the proto has cache read, cache write or cost fields.

### 6. Codex and OpenCode specs

Codex, `specs/codex.yaml` (headless-only):
- Conversation: none declared, so `ConversationSlot::Worktree` (derived). The headless argv uses `resume <session_id>` with `session_id` from the codex-jsonl adapter (`thread_id`), `fresh_session: agent`.
- Home: no `home:` section, so the default `HomeSection::Session`.
- `placements: [host]` (`:106`), so it is never jailed. The `run:` template is just `{binary}`, with the comment "which no session type launches".
- `for_session_type("codex-cli")` would technically resolve (pty transport) and run bare `codex`; the comment says no session type launches it.
- No `CODEX_HOME` and no `XDG_*` set anywhere in the repo (grep: none). `pass_env` is `[OPENAI_API_KEY]`. The headless env sets only `BROWSER` and `TDDY_CODEX_OAUTH_OUT`.
- Codex therefore runs as the OS user (host) with HOME equal to the passwd home, so rollouts land at the default `~<os_user>/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. This follows from the Codex CLI's default of `$CODEX_HOME` or `~/.codex`, which is not stated in this repo. The tddy-coder workflow backend runs `codex exec` in the tddy-coder process, whose HOME is the passwd home. The daemon-run coding subagent runs it in the daemon with `AgentEnvBase` passthrough, which includes HOME and `XDG_*`.

`codex-acp`, `specs/codex-acp.yaml`:
- `run.transport: acp`, `placements: [host]`, no `home:`, no conversation slot.
- Same home story (the passwd or daemon HOME). No `CODEX_HOME`.

OpenCode, `specs/opencode.yaml`:
- Conversation: none declared, so `Worktree`. It resumes with `opencode --continue`, "one session per worktree".
- `home: { kind: session }`, so the jail HOME is `<session_dir>/sandbox/.work/home`.
- `placements: [host, jail]`, `pass_env: [ANTHROPIC_API_KEY, OPENAI_API_KEY]`.
- No `XDG_DATA_HOME` is set. The jail env is `HOME` plus `TMPDIR`, `TERM` and `PATH`, plus the daemon's `session.env` (non-`TDDY_`) and `OPENCODE_CONFIG_CONTENT` / `OPENCODE_PROVIDER_KEY` (the route).
- The jailed `opencode.db` therefore lands at XDG default `<session_dir>/sandbox/.work/home/.local/share/opencode/opencode.db`. This is inferred from OpenCode's default data dir; no code in the repo names `opencode.db`.
- For a host (non-jailed) OpenCode PTY session the home is the passwd home, so `~<os_user>/.local/share/opencode/opencode.db`. On host the `agent-env.yaml` base passes through `XDG_*` from the daemon's env, so `XDG_DATA_HOME` would apply if the daemon had it set.
- OpenCode TUI jailed start refuses without a provider route (`route.required_in_jail: true`).

### Gaps and observations for the plan
- `HostAgent`/`CodingAgentSpec` already carries `transcript_usage: bool`. It is read only by `tddy-sandbox-app`; the daemon does not read it.
- The daemon has no accounting code today. `tddy-agent-launch` and `LaunchSessions` are the daemon's launch library.
- There is no recorded "agent home" for resumes or restarts; recompute it (`jail_home` for the jail, `home_dir_for_user(os_user)` for the host).
- The shared `homes/claude` dir contains transcripts of every Claude jailed session, so match by `session_id` (the existing readers already do).
- Dedupe by `message.id` is needed for Claude transcripts (see the parent's discovery doc, which measured 10 assistant lines versus 6 unique ids).
- `Resume` of a Claude session keeps the same `session_id`, so the same transcript file keeps growing.