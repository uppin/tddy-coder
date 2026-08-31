# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N] [--indexing-budget SECONDS]`
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--indexing-budget SECONDS]`
- `anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]`
- `verify --against <git-ref>`

Plans hold intents only — no source text. Unsupported operations are hard errors.
