# 2026-10-08 — a cluster move strands the `*_tests.rs` modules that test only the moved code; they are hand-moved

**Category:** Engine failure plus manual fixes (hand moves, in their own commit)
**Source:** #carve 21/21 (PR #536), R9: the launch cluster → `tddy-agent-launch`. The changeset says tests "move with their code"; the engine moves modules, not the `#[cfg(test)] mod x_tests;` files declared beside them.

## What the engine did

`move_cluster_to_crate` left behind every test module declared in the origin. Five failed to compile in lifecycle (they `use super::…` names that are now in another crate), one failed in the destination:

| Test file | Moved by hand to | Why |
|---|---|---|
| `connection_service/claude_cli_spawn/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs` | `tddy-agent-launch/src/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs` | declared by the moved `claude_cli_spawn_steps.rs` (`E0583` in the destination) |
| `connection_service/conversation_spawn_wiring_tests.rs` | `tddy-agent-launch/src/conversation_spawn_wiring_tests.rs` | tests only the moved `conversation_spawn` |
| `connection_service/host_session_socket_tests.rs` | `tddy-agent-launch/src/host_session_socket_tests.rs` | tests only the moved `host_session_socket` |
| `connection_service/session_acting_identity_tests.rs` | `tddy-agent-launch/src/session_acting_identity_tests.rs` | tests only the moved `session_acting_identity` |

`stack_child_spawn_tests.rs` was tried in the destination and put back: it builds a `DaemonSessionHost`, so it stays in lifecycle (its names go through `pub use` facades after widening `StackChildSpawnHandler`, `GrillMeConversationSpawnHandler` and their fields).

## Hand fixes after the moves

Four `use` lines: `super::recipe_enables_conversation_spawn` → `crate::conversation_spawn::…`; `crate::user_sessions_path::username_for_uid` → `tddy_session_activity::user_sessions_path::…`;
`crate::connection_service::AttachmentProgressSink` → `tddy_session_files::attachment_progress::…`; and a `[dev-dependencies] tempfile = "3"` line in the destination's manifest.

## What the engine should do

Treat a `#[cfg(test)] mod x_tests;` whose `super::` names are all in the moving set as part of the move (the file and its declaration), or list it in `check --deep` as stranded. Delete this file with that fix.
