# Participant host

`tddy-coder` serves its RPC surface as a room participant through a port, so the library names no
LiveKit crate. The binary crate `tddy-coder-cli` (binary name `tddy-coder`) supplies the transport.

## The port

`tddy_coder::participant_host`:

- `SessionParticipantHost::serve(ParticipantJoin, MultiRpcService, ParticipantOptions) ->
  ServedParticipant` hosts the surface in a room.
- `ParticipantJoin` is the URL, the identity and a `ParticipantAuth`: `Credentials { api_key,
  api_secret, room, ttl }` (the host mints for `room`, and re-mints on reconnect) or `Token(String)`
  (the room is the one the token was minted for).
- `ParticipantOptions` carries the metadata `watch::Receiver`, the codex-OAuth watch path, the
  projects-registry directory, `reconnect`, and the shutdown flag.
- `ServedParticipant` is the participant's task. It ends when the room does or on shutdown; dropping
  it does not stop the participant. `ended()` resolves when it has ended.
- `SessionParticipantHost::token_provider(api_key, api_secret, ttl)` returns a provider that mints
  room tokens for other participants, for the token service the coder serves next to its surface.
  The default refuses.
- `NoParticipantHost` refuses every call, naming the identity and URL (or the missing minting).
  `tddy-demo` wires it, so a LiveKit flag on the demo is refused when it asks to join.

## Join policy

`participant_join_from_cli(url, room, identity, token, credentials)` is the policy both of `run.rs`'s
modes ask for: with API credentials the coder re-mints for its room and reconnects
(`PARTICIPANT_TOKEN_TTL`, six hours); with only a token it serves once, with that token.
`serve_until_ended` serves and waits, logging a refusal rather than returning it, so the workflow
keeps running without the room.

`run_main(args, host)` and `run_with_args(args, shutdown, host)` take the host; every join site in
`run.rs` (daemon and TUI, with and without credentials) goes through it. The daemon-spawned session
participant (`session_participant::spawn_session_participant_with`) joins once, under its session
identity, with its token.

## The LiveKit implementation

`tddy_coder_cli::LiveKitParticipantHost` implements the port over `tddy_livekit::LiveKitParticipant`:

| Join | `reconnect` | Behaviour |
|---|---|---|
| `Credentials` | true | `run_with_reconnect_metadata`, rejoining on its own |
| `Credentials` | false | mints once, connects once |
| `Token` | false | connects once, publishes metadata from a watcher, ends with the room or on shutdown, logs `READY` |
| `Token` | true | refused: a fixed token cannot be re-minted |

The crate ships a library target (so integration tests can import the host) and the `tddy-coder`
binary. Tests that run the built binary live in its `tests/`, because `CARGO_BIN_EXE_tddy-coder`
exists only for the package that owns the binary.

## Metadata merge helper

`merge_participant_metadata_json` is defined in `tddy-changeset`'s `session_participant_metadata`
module, re-exported by `tddy-core` and by `tddy-livekit`.
