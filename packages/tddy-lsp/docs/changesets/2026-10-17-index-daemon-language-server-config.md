# 2026-10-17 — A server that cannot be started is refused as not found

**Type:** Fix

`LspServerBody::client_tx` carries `Result<Arc<LspClient>, LspError>`. A failed spawn sends
`LspError::ServerNotFound("<program>: <io error>")` (`NotFound`) or `LspError::Io` (anything else) before the body
ends, and `LspRegistry::get_or_spawn` returns it; `ServerExited` is left to a server that started and went away.
Pinned by `spawn_observer_test::a_server_program_that_does_not_exist_is_refused_as_not_found_naming_the_program`.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-17-index-daemon-language-server-config.md).
