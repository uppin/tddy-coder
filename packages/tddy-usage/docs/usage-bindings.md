# Usage bindings — evaluation

## Modules

| Module | Holds |
|---|---|
| `section` | `UsageSection`, `UsageSource` (`format`-tagged; `jsonl`), `JsonlSource`, `MainSpec`, `ChildrenSpec`, `NameSpec`, `RecordSpec`, `FieldEq`, `Aggregate`, `CounterMap`; `validate_usage_section` |
| `template` | `PathTemplate` — `{name}` substitution, `{{`/`}}` literal braces, `TemplateError::{Unknown, Unset}` |
| `field_path` | `FieldPath` — a dotted path of JSON object keys; empty paths and segments are refused at parse |
| `jsonl` | `fold_files`: filter by `where`, dedupe by key (last line wins), `sum` the counters and cost, last `model`, `turns` = kept records |
| `tree` | `conversation_rows`: render and glob `main.path`, merge every match into the main row, then follow `children` per file |
| `gather` | `UsageContext`, `account`, `gather_session_usage`, the `accounting.json` reader |
| `token_accounting` | the wire record (moved from `tddy-agent-backend`, which re-exports it) |

## Rules

- **Templates.** Main templates bind `{agent_home}`, `{session_dir}`, `{conversation_id}`,
  `{worktree}`; children templates bind the file's `{dir}` and `{stem}`. A variable outside the
  position's set is refused at load; a known variable with no value at runtime (`TemplateError::Unset`)
  matches no files. Values are glob-escaped when the result is a pattern.
- **Tree.** Depth guard 16, a visited-path set against cycles, siblings ordered by id, each followed
  by its own descendants. A child's `parent_id` is its parent's id.
- **Zero row.** With no main file the main conversation is still one row, on `ctx.fallback_model`.
  No conversation id: the source yields nothing.
- **Counters are disjoint.** `total_tokens = input + cache_read + cache_write + output + reasoning`.
- **Robustness.** A non-JSON line is skipped; an unreadable file or a malformed `accounting.json`
  logs a warning and contributes nothing — never an error.
- **No agent vocabulary.** `tests/no_agent_vocabulary.rs` scans `src/` for agent names.
- `ConversationRecord::new` builds a top-level record with only fresh input and output (the shape a
  tddy subagent reports).
