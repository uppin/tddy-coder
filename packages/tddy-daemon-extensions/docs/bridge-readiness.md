# Bridge start report

`tddy_daemon_extensions::bridge_readiness` is the protocol a bridge (`tddy-vnc`, `tddy-rdp`) uses to
tell the daemon how its start went. It is one JSON line on the bridge's stdout. The daemon reads it
(`tddy-screen-sharing` `await_bridge_readiness`) to fail a start with the desktop's reason, or to
report the desktop's real size. The bridge writes it from `tddy-screenshare`'s `run_bridge`, which
re-exports these items at its crate root, so a bridge client names `tddy_screenshare::AuthenticationRefused`.

The protocol lives in the extension-ports crate so that the daemon reads it without depending on the
bridge crate, which is LiveKit's (`tests/livekit_is_a_satellite.rs`).

Product behaviour: [The bridge's start report](../../../docs/ft/web/screen-sharing-sessions.md#the-bridges-start-report).
Ports this crate also holds: [Extension ports](extension-ports.md).

## Types

| Item | Shape |
|---|---|
| `BridgeReadiness` | `Ready { width, height }` or `Failed { kind: FailureKind, message }`, `serde(rename_all = "snake_case")` |
| `FailureKind` | `Authentication`, `Connect`, `LiveKit` (on the wire: `authentication`, `connect`, `live_kit`) |
| `write_readiness(out, &readiness)` | one line, then a flush: the daemon is blocked on it |
| `read_readiness(line)` | parses one line |
| `AuthenticationRefused(reason)` | the marker error a client's `connect` puts in its error chain when the desktop refused the sign-in |

## How `run_bridge` (`tddy-screenshare`) reports

`run_bridge` splits into `start_bridge` (connect, wait for the framebuffer size, join LiveKit,
publish the track, request the first frame) and the pump loop. Exactly one line is written:

- `Ready` with the framebuffer size, once `start_bridge` succeeds, before the pump loop starts.
- `Failed`, when `start_bridge` fails. The bridge then exits with that error. `classify` finds
  `AuthenticationRefused` anywhere in the error chain (by type, never by the server's wording) and
  reports `Authentication` with the desktop's reason. Otherwise it reports the stage's kind (`Connect`
  for the desktop side, `LiveKit` for the room and track) with the whole error chain as the message.

Stdout carries nothing else; a bridge logs to stderr.

## Who tags a refusal

| Bridge | Refusal | Tagged where |
|---|---|---|
| `tddy-vnc` | `SecurityError::Refused { reason }` from `security::negotiate` (VNC Authentication, VeNCrypt, Apple Remote Desktop) | `vnc_client::tag_refused_sign_in` |
| `tddy-rdp` | a CredSSP (`ConnectorErrorKind::Credssp`) connector error | `RdpClient::connect` |

Tests: `bridge_readiness.rs` unit tests (`a_ready_line_round_trips`, `a_failure_line_names_its_kind`),
`tddy-vnc/tests/vnc_security_integration.rs` `a_refused_vnc_password_is_reported_as_an_authentication_failure`,
`tddy-rdp/tests/rdp_nla_integration.rs` `a_wrong_password_on_an_nla_server_is_tagged_as_a_refused_sign_in`.
