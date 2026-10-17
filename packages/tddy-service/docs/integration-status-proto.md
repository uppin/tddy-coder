# Integration status (`daemon_config.proto`)

`daemon_config.DaemonConfigService.GetIntegrationStatus` reports the live state of each optional
integration the daemon has wired (LiveKit is the first). It is served by the daemon's own config
service rather than by the integration, so it stays readable when the integration is the thing that
is down.

| Message | Fields |
|---|---|
| `GetIntegrationStatusRequest` | `session_token`: the daemon access token, gated exactly like `GetConfig`. |
| `GetIntegrationStatusResponse` | `integrations`: one `IntegrationStatus` per wired integration, ordered by `id`; empty when none is wired. |
| `IntegrationStatus` | `id` (stable, e.g. `livekit`), `label` (e.g. `LiveKit`), `state`, `error` (the last failure; set only for `UNREACHABLE`), `missing_fields` (set only for `MISCONFIGURED`), `since_unix_ms` (when the integration entered that kind of state). |

`IntegrationState`: `UNSPECIFIED` (0), `DISABLED` (1: switched off, or not configured at all),
`MISCONFIGURED` (2: a required field is missing), `CONNECTING` (3), `CONNECTED` (4) and
`UNREACHABLE` (5: configured and not answering).

A daemon started without an integration status board answers `failed_precondition`.

Where the states come from: [extension ports](../../tddy-daemon-extensions/docs/extension-ports.md#the-integration-status-board-integration_status)
(the board), [LiveKit status reporting](../../tddy-daemon-livekit/docs/livekit-service.md#status-reporting)
(what LiveKit reports and when) and [the daemon endpoint](../../tddy-daemon/docs/daemon-endpoint.md)
(the wire mapping). Product behaviour: [Daemon settings](../../../docs/ft/daemon/daemon-settings.md).
