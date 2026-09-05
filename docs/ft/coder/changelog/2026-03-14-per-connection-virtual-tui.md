# 2026-03-14 — Per-Connection Virtual TUI

- **Presenter view decoupling**: `connect_view()` → ViewConnection (state snapshot + event_rx + intent_tx). NoopView for headless/daemon.
- **VirtualTui**: Headless ratatui per connection; CapturingWriter headless(); event subscription, key parsing.
- **TerminalServiceImplPerConnection**: One VirtualTui per StreamTerminalIO call. Daemon with LiveKit exposes TerminalService (per-connection VirtualTui) instead of EchoService.
- **E2E**: two_grpc_clients_get_independent_terminal_streams, two_livekit_clients_get_independent_terminal_streams.
- **Packages**: tddy-core (ViewConnection, NoopView), tddy-tui (VirtualTui), tddy-service (TerminalServiceImplPerConnection, view_connection_factory), tddy-coder (run_daemon wiring), tddy-e2e (spawn helpers, virtual_tui_sessions, terminal_service_livekit).
