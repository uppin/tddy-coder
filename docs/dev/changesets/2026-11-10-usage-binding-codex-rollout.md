# 2026-11-10 — Codex usage from its rollout files, spawned threads nested

**Type:** Feature

`#usage-binding 4/5`, PR [#893](https://github.com/uppin/tddy-coder/pull/893), stack
[#895](https://github.com/uppin/tddy-coder/stack/895). Feature:
[session-token-accounting.md](../../ft/coder/session-token-accounting.md).

## What changed

- **`tddy-usage`** — the `jsonl` format gains:
  - `aggregate: last` (`Aggregate::Last`);
  - `identity: { id, parent }` (`IdentitySpec`): a file names its own conversation, read once from its
    first 50 lines;
  - `main.match: { field, equals }` (`MainMatch`): the newest parentless file whose header field equals
    the rendered template is the main conversation;
  - `children_by: parent_field` (`ChildrenBy`): children are the files declaring a conversation as
    their parent, recursively;
  - counter expressions (`CounterExpr`, in `counter_expr.rs`): `a - b`, `a + b`. `CounterMap`'s fields
    change from `Option<FieldPath>` to `Option<CounterExpr>`, which still parses every plain path.
  - `record.model` is now read from every line, including those `where` drops.
  - `validate_usage_section` refuses a malformed expression, `children_by` or `main.match` without an
    `identity`, and an `identity` beside `children`; its per-source checks moved to `check_source` /
    `check_identity`.
- **`tddy-coding-agent-specs`** — `codex.yaml` and `codex-acp.yaml` bind
  `{agent_home}/.codex/sessions/*/*/*/rollout-*.jsonl`.

## Deviations from the plan

- **The shipped key is `children_by: parent_field`**, beside `children`, not
  `children: { by_parent_field: true }`: additive, since `ChildrenSpec` keeps its required `path`/`id`.
- **No ACP cost bindings** (handed down from `3/5`): no recorded `usage_update` exists —
  [todo](../todo/2026-11-11-acp-usage-update-cost-unbound.md).
- **Known gap: the fixtures are synthetic.** `codex_binding_acceptance.rs` builds rollouts shaped like
  Codex CLI 0.167's (header, `turn_context`, `token_count`); no real user rollout was read, so the
  binding is untested against a live Codex session.
- **The glob is `*/*/*`**, three date levels, which walks every rollout the user ever wrote. Narrowing
  it by the session's start date is open if the walk measures slow.
- **`context_tokens` stays 0** for Codex too (`TODO(usage-binding)`): occupancy needs one record's
  input, not a whole-counter aggregate.

## Wrap fixes

- `fold_files` keeps `keep`, `passes_where` and `fold` helpers (`#usage-binding 3/5`'s shape) with
  `last` folded in.
- `identified_rows` reads each file's header once (`header_values`) instead of once per field.
- `validate_usage_section`'s per-source checks became `check_source` / `check_identity` (cyclomatic 19 → split into
  three functions).
- Unit tests added for `aggregate: last` and `sum`, the identity/children conflict, and the
  one-owner-per-counter refusal (the last had 0% coverage inside `tddy-usage`; CRAP 42 → covered).

## Code issues

- No code-issue record added; none existed in `tddy-usage` or `tddy-coding-agent-specs`.
- No file passes 500 production lines (`tree.rs` 338, `section.rs` ~270, `jsonl.rs` ~165).
  `tddy-usage`'s `section.rs`, `tree.rs`, `jsonl.rs` and `lib.rs` are also modified by `opencode-sqlite`;
  none needed a split.
