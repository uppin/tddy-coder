# Initial discovery — `#orchestra` sweep

## Conclusions

- Code-issue records: the stack added none and closed one (`oversized-file-spec.md`, closed by #980's `spec/variant.rs` split). Measured by `git diff --name-status <pre-stack>..#996 -- 'packages/*/docs/code-issues/'`.
- `/analyze-code-issues` was not run on n5–n8's crates during the stack; the developer chose to skip it here too.
- Todo entries the stack added: seven; one (`session-file-tools-are-not-forwarded-to-a-peer-host`) is resolved by #996, six were deferred at the TODO gate.
- Sweep items from the stack's pipeline notes: duplicate `orchestrator_caller`; `tddy-tools` test reading `TDDY_REMOTE_*`; `echo-unary.test.ts` fixed port `8887`.

## Exploration 1

- `git grep -n "fn orchestrator_caller"` → `exec_tool/base_freshness.rs:68` (`pub(super)`; used by `base_freshness.rs`, `lsp_roots.rs:28`, `path_access.rs:34`) and `exec_tool/session_files.rs:99` (private). Bodies differ only in the read-error wording ("record" vs "role").
- `exec_tool/project_home/caller.rs` holds `project_home_caller` — the natural home.
- `packages/tddy-tools/src/server.rs:2821` — the test; `tests/approval_prompt_deny_paths.rs:18` already clears the four `TDDY_REMOTE_*` variables for its suite.
- `packages/tddy-rust-typescript-tests/tests/echo-unary.test.ts:35` — `const PORT = 8887;`.
