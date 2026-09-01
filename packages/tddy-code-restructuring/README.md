# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

**Feature doc:** [docs/ft/coder/rust-code-restructuring.md](../../docs/ft/coder/rust-code-restructuring.md)

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N] [--indexing-budget SECONDS]`
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--indexing-budget SECONDS]`
- `anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]`
- `verify --against <git-ref>`

Plans hold intents only — no source text (`text` / `code` / `content` refused). Unsupported operations are hard errors.

## Operations (v1)

`extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`, `to_file`), `extract_module_to_file`, `extract_trait`, `inline_method`.
