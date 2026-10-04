# LiveKit testkit

`tddy-livekit-testkit` starts a LiveKit server for tests and hands out what a test needs to talk to
it. `LiveKitTestkit::start()` launches a Docker container, or reuses the server named by
`LIVEKIT_TESTKIT_WS_URL`; the API key and secret (`devkey` / `secret`) are shared by every test.

## API

| Method | Returns |
|---|---|
| `start()` | a testkit bound to a running server |
| `get_ws_url()` | the server's WebSocket URL |
| `generate_token(room, identity)` | an access token for one identity in one room |
| `remove_participant(room, identity)` | removes a participant from a room |
| `unique_room(prefix)` | a room name no other test uses |

## Room names

`unique_room(prefix)` returns `<prefix>-<hex nanos>-<pid>-<counter>`: the nanosecond clock separates
runs, the process id separates processes, and an atomic counter separates calls within one process.
The result holds only letters, digits and `-`, so it is a valid LiveKit room name, and needs no extra
dependency. The prefix is the room's purpose, so a room left behind by an aborted test can be
attributed.

Every test that starts the testkit names its room this way, so two tests never share a room whether
they run in one process, in two, or against one shared server. A test whose participants must meet in
a room builds the name once (per test, or per process in a binary whose tests share one fixture) and
passes it to every token. Identities are room-scoped and may repeat across tests.

## Guard test

`tests/livekit_tests_use_unique_rooms.rs` walks every package's `tests/` directory and fails, naming
the file, line and constant, when a file that starts the testkit names a room with a string literal.
Files that mint tokens but never connect, such as the testkit's own integration test, are allowlisted
explicitly.

## Roster reads

`ListParticipants` for a room that does not exist returns an empty list rather than an error, so a
roster read that races a room closing does not fail.
