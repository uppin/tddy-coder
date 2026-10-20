# Desktop inspect line

`tddy_daemon_extensions::desktop_inspect` is the protocol a bridge (`tddy-vnc`, `tddy-rdp`) uses to
tell the daemon which sign-in methods a desktop advertises, read without signing in. The daemon runs
the bridge binary with `--inspect` (`INSPECT_FLAG`), writes an `InspectTarget` JSON to its stdin, and
reads one `InspectReport` line from its stdout (`tddy-screen-sharing` `inspect_host_desktop`).

The protocol lives in the extension-ports crate for the same reason as
[the bridge start report](bridge-readiness.md): the daemon reads it without depending on the bridge
crate, which is LiveKit's (`tests/livekit_is_a_satellite.rs`). The bridge half — the argument
parsing (`BridgeMode::from_args`: no arguments bridges, `--inspect` inspects, anything else is
refused by name) and the run itself (`run_inspect::<C: ScreenSharingClient>`, which calls
`C::inspect(host, port)`) — is in `tddy-screenshare`, which re-exports these items, so a bridge
names `tddy_screenshare::AdvertisedMethod`.

Product behaviour: [The desktop credential is asked for, and kept only on Remember](../../../docs/ft/web/screen-sharing-sessions.md#the-desktop-credential-is-asked-for-and-kept-only-on-remember).

## Types

| Item | Shape |
|---|---|
| `InspectTarget` | `{ host, port }` — an address and nothing a sign-in would need |
| `AdvertisedMethod` | `None`, `VncPassword`, `VencryptPlain`, `VencryptTlsPlain`, `AppleRemoteDesktop`, `RdpTls`, `RdpNla`, `UnsupportedVnc { security_type }`; `serde(rename_all = "snake_case")` |
| `InspectReport` | `{ methods: Vec<AdvertisedMethod> }`, in the order the desktop advertised them |
| `write_inspect_report(out, &report)` | one line, then a flush |
| `read_inspect_report(line)` | parses one line |

A line, for a VNC desktop offering Apple Remote Desktop beside Tight (type 16):
`{"methods":["apple_remote_desktop",{"unsupported_vnc":{"security_type":16}}]}`.

## What each bridge reads

- **`tddy-vnc`** (`security::inspect`): the RFB version exchange and the security-type list. `None`,
  VNC Authentication and Apple Remote Desktop map directly, and any other type is
  `UnsupportedVnc` by number. VeNCrypt (19) is not a method an operator chooses. When offered (not
  over RFB 3.3), the bridge selects it **after** reading every other type — selecting it commits the
  connection — and appends the subtypes it can sign in with (Plain 256, TLSPlain 259). Nothing else
  is selected and no credential is sent. Bounded by the sign-in timeout.
- **`tddy-rdp`** (`RdpClient::inspect`): an X.224 Connection Request offering TLS, CredSSP and
  CredSSP-EX. A Confirm selecting CredSSP, or failing with `HYBRID_REQUIRED_BY_SERVER`, is `RdpNla`.
  One selecting TLS, or failing with `SSL_REQUIRED_BY_SERVER`, is `RdpTls`. Anything else is an
  error. Bounded by 30 s.

An inspection that fails — the desktop unreachable, or the protocol broken — is an error: the bridge
exits non-zero with the reason on stderr and writes no report.
