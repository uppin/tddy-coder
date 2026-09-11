# 2026-09-11 — `session_files.proto`, `types.proto`, and 22 rpcs leave `connection.proto`

`connection.proto` declares **50** rpcs. Families I, J, K, R and S left: nine to
`terminal_session.TerminalSessionService` (`packages/tddy-terminal-rpc/proto/`) and thirteen to the
new `session_files.proto`, which imports `types.proto`.

**No field number was reserved, and none needed to be.** Nothing that stayed referenced a moved
message, so the departed request, response and payload messages went whole; and protobuf has no
`reserved` for service methods. A header note on `connection.proto` records the 22 vacated
coordinates so they are not re-declared — that note is the record, not a field-number range.

`types.proto` holds **exactly one** declaration, `HostDocumentScope`, because two separately served
services reach it: `connection.ConnectionService`'s `StartSession` names the staged attachments to
materialise and each carries a scope, and `session_files.SessionFilesService`'s `ReadHostDocument`
resolves one. A test pins the file at one declaration, so anything added later has to clear the same
bar.

`sandbox.proto` owns a `SandboxTerminalOutput` of its own rather than borrowing a terminal message.
Naming `terminal_session`'s would be a dependency cycle — `tddy-terminal-rpc` depends on this
crate. The four fields it drops (`acked_input_offset`, `start_offset`, `end_offset`, `at_oldest`)
are capture-ring and input-ack metadata the jail never set and the host relay never read, and they
are `reserved` so a later field cannot claim one and collide with what an older runner still
encodes. The three survivors keep their numbers, so the bytes on that hop are unchanged.

`to_tonic_status` and `to_rpc_status` live here. Generated tonic adapters land in this crate's and
`tddy-terminal-rpc`'s `OUT_DIR` and neither depends on `tddy-daemon`, where the pair used to be.
(`tddy-rpc` carries its own conversion but pins tonic 0.11 against everything else's 0.12, which is
why a separate one exists at all.)

`tests/unbundle_service_split.rs` carries the two sweeps: the rpc count, a literal each node bumps
because a count derived from its own input asserts nothing; and
`no_source_converts_between_the_two_terminal_message_sets`, widened from one needle under
`tddy-daemon/src` — satisfiable by deleting a doc comment — to both message names across
`tddy-daemon` and `tddy-coder`.

**Note for the reflection surface:** the descriptor-only pass in `build.rs` can name only this
crate's protos, so `terminal_session.TerminalSessionService` is listed by `list_services` but its
`file_containing_symbol` lookup answers `not_found`. Recorded in
[`docs/dev/todo/2026-09-11-reflection-cannot-serve-descriptors-for-protos-outside-tddy-service.md`](../../../../docs/dev/todo/2026-09-11-reflection-cannot-serve-descriptors-for-protos-outside-tddy-service.md).

Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
