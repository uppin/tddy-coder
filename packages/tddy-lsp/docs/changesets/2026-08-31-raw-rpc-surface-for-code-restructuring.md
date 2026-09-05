# 2026-08-31 — raw RPC surface for code restructuring

**Type:** Feature

`LspClient::request_raw` and `notify_raw` let `tddy-code-restructuring` call rust-analyzer assists through the existing long-running client without spawning a second server. Typed assist APIs (`codeAction`, `rename`, `semanticTokens`, progress) remain a follow-up. Feature [rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-lsp)
