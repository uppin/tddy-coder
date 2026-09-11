# 2026-09-11 — gRPC reflection lists `terminal_session.TerminalSessionService` but cannot describe it

**Category:** Known gap
**Source:** `#unbundle` node 6, [#475](https://github.com/uppin/tddy-coder/pull/475), changeset
[`2026-09-11-unbundle-session-io-services`](../changesets/2026-09-11-unbundle-session-io-services.md)

`packages/tddy-service/src/reflection_service.rs` serves descriptors out of a `FileDescriptorSet`
embedded at build time by a descriptor-only pass in `packages/tddy-service/build.rs`. That pass can
only name protos in `tddy-service`'s own `proto/` directory.

`list_services` answers from the **host's registry**, not from the descriptor set, so the two can
disagree — and they do:

| Service | Listed | Describable |
|---|---|---|
| `session_files.SessionFilesService` | yes | yes (`proto/session_files.proto` is in the pass) |
| `terminal_session.TerminalSessionService` | yes | **no** — the proto is `packages/tddy-terminal-rpc/proto/terminal_session.proto` |

So the [RPC Playground](../../ft/daemon/rpc-playground.md) shows the terminal service in its tree and
`file_containing_symbol` answers `not_found` when a developer expands it. Nine methods, including the
only bidirectional one in the whole surface, are unreachable from the playground.

**The obvious fix does not work.** `tddy-terminal-rpc` depends on `tddy-service`, so adding
`../tddy-terminal-rpc/proto/terminal_session.proto` to `tddy-service`'s descriptor pass is a build
dependency in the wrong direction.

## Options

- **Compose the set at the host.** `reflection_entry_from` already takes the service names; give it
  the descriptor bytes too, and let each host concatenate the `FileDescriptorSet`s of the crates it
  registers. `tddy-terminal-rpc`'s `build.rs` would emit its own with
  `file_descriptor_set_path`. This is the only option that scales as more services move out of
  `tddy-service`.
- **Serve reflection from a crate that depends on both.** Moves the cycle rather than removing it,
  and puts a new crate between every host and reflection.
- **Accept it and say so in the UI.** The playground could render a service it cannot describe as
  explicitly undescribable rather than as an empty tree, which is a smaller change and leaves the
  methods uninvokable.
