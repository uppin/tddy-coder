# Usage bindings — evaluation

## Modules

| Module | Holds |
|---|---|
| `section` | `UsageSection`, `UsageSource` (`format`-tagged; `jsonl`, `sqlite`), `JsonlSource`, `SqliteSource`, `RootMatch`, `MainSpec`, `MainMatch`, `IdentitySpec`, `ChildrenBy`, `ChildrenSpec`, `NameSpec`, `RecordSpec`, `FieldEq`, `Aggregate` (`Sum`, `Last`), `CounterMap`; `validate_usage_section` |
| `counter_expr` | `CounterExpr` — `path (('+'\|'-') path)*`, parsed at load (serde `try_from = "String"`), `evaluate` over one line |
| `template` | `PathTemplate` — `{name}` substitution, `{{`/`}}` literal braces, `TemplateError::{Unknown, Unset}` |
| `field_path` | `FieldPath` — a dotted path of JSON object keys; empty paths and segments are refused at parse |
| `jsonl` | `fold_files`: filter by `where`, dedupe by key (last line wins), fold the counters and cost by `aggregate` (`sum`, or `last` = the last record carrying each), last `model` over **every** line, `turns` = kept records |
| `tree` | `conversation_rows`: render and glob `main.path`, merge every match into the main row, then follow `children` per file; with an `identity`, `identified_rows` instead (header-named conversations) |
| `sqlite` | `conversation_rows` for a `sqlite` source: a read-only connection, the root row, then children depth-first (`Walk`); `is_identifier` / `named_identifiers` serve load-time validation |
| `gather` | `UsageContext`, `account` (one row per conversation: `merge_sources` over the sources' rows), `gather_session_usage`, the `accounting.json` reader |
| `registry` | `RegisteredConversation`, `register_conversation` / `registered_conversations` (`<session_dir>/usage-conversations.jsonl`, `CONVERSATION_REGISTRY_FILE`) |
| `context_file` | `UsageContextFile` (`spec_id`, `agent_home`, `conversation_id`, `worktree`, `model`), `write_usage_context` / `read_usage_context` (`<session_dir>/usage-context.json`, written to a sibling and renamed), `gather_session_usage_for` |
| `token_accounting` | the wire record (moved from `tddy-agent-backend`, which re-exports it) |

## Rules

- **Templates.** Main templates bind `{agent_home}`, `{session_dir}`, `{conversation_id}`,
  `{worktree}`; children templates bind the file's `{dir}` and `{stem}`. A variable outside the
  position's set is refused at load; a known variable with no value at runtime (`TemplateError::Unset`)
  matches no files. Values are glob-escaped when the result is a pattern.
- **Tree.** Depth guard 16, a visited-path set against cycles, siblings ordered by id, each followed
  by its own descendants. A child's `parent_id` is its parent's id.
- **Identity.** With `identity`, each file's id and parent are read once from its first 50 lines
  (`HEADER_LINES`; the first line holding the field). The main conversation is the newest
  (modification time, then path) file with no parent whose `main.match.field` equals the rendered
  `equals` (a template over the main variables, not glob-escaped); none qualifying yields no rows.
  `children_by: parent_field` then nests every file declaring a conversation as its parent, depth
  guard 16, each conversation id visited once, siblings by id. `children_by` and `main.match` without
  an `identity`, and an `identity` beside `children`, are refused at load.
- **Counter expressions.** A missing or non-numeric operand is 0; a line carrying none of an
  expression's fields has no value (so `last` skips it); a negative result saturates at 0 with a
  warning. A key containing `+` or `-` cannot appear in an expression.
- **`last`.** Per counter, the last kept record that has a value for it (a record without the field
  does not reset it to 0); the cost is the last one reported.
- **Zero row.** With no main file (or, for `sqlite`, no store or root row) the main conversation is still one row, on `ctx.fallback_model`.
  No conversation id: the source yields nothing.
- **One owner per counter.** `validate_usage_section` refuses two sources mapping the same counter
  (or both mapping `cost`); `merge_sources` then adds rows with an equal id, which is the owning
  source's figure, takes the first `cost` seen and the first non-empty model.
- **Sqlite.** Table and column names go into SQL, so each is checked at load as a plain identifier
  (`check_sqlite`) and quoted; the values (`root.equals`, a parent id) are bound parameters. The
  connection is `SQLITE_OPEN_READ_ONLY | NO_MUTEX` with a 200 ms busy timeout. Rows are presented to
  `CounterExpr` as a JSON object of the selected columns (a blob reads as null). The root is the newest
  (`newest_by`) row matching `root`, with a null parent when the source has a `parent` column;
  children follow `parent = id` under the depth guard 16 and a visited-id set. A store that cannot be
  opened, prepared or read returns `None` (the caller keeps its snapshot); a missing file or root row
  returns the zero main row. `turns` is 0.
- **Counters are disjoint.** `total_tokens = input + cache_read + cache_write + output + reasoning`.
- **Robustness.** A non-JSON line is skipped; an unreadable file or a malformed `accounting.json`
  logs a warning and contributes nothing — never an error.
- **No agent vocabulary.** `tests/no_agent_vocabulary.rs` scans `src/` for agent names.
- `ConversationRecord::new` builds a top-level record with only fresh input and output (the shape a
  tddy subagent reports).

## The recorded usage context

A session's launch knows where its agent keeps its files; whoever accounts the session later does
not. So every launch, resume and relaunch writes `usage-context.json` next to `.session.yaml`
(`tddy-agent-launch::usage_context`, `tddy-spawn`'s workflow spawn), and the daemon's usage stream
(`tddy-session-activity`) evaluates a session with `gather_session_usage_for(session_dir, binding_for)`:
it reads the context, asks `binding_for(spec_id)` for the spec's binding, and builds the
`UsageContext` from the file (`agent_label` = the spec id, `fallback_model` = the recorded model). A
session with no context, or one that does not parse (logged), is evaluated with no binding: only its
tddy subagent rows.

## Registered conversations

A coding subagent the daemon opens for a session is not in the recorded context, so the opener
appends a `RegisteredConversation` line to `usage-conversations.jsonl` (one `write_all` under
`O_APPEND`, safe for concurrent opens; the last line per conversation id wins, a resume re-registers).
`gather_session_usage_for` then evaluates each registered conversation **not already among the main
rows** with `binding_for(spec_id)` and its own context (`agent_label` = the registration's label),
setting its top row's `parent_id` to the registration's parent. A spec with no binding yields no rows;
an unparseable line is logged and skipped.
