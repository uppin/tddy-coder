# 2026-03-11 — Terminal Streaming via gRPC

**Type:** Feature

Proto: StreamTerminal RPC, StreamTerminalRequest, TerminalOutput. TddyRemoteService.with_terminal_bytes; stream_terminal subscribes to broadcast, streams bytes to clients. DaemonService stub returns empty stream. Integration tests: stream_terminal_returns_bytes, streamed_bytes_contain_ansi_sequences, multiple_clients_receive_same_stream, stream_terminal_returns_empty_stream_when_no_terminal_bytes. (tddy-grpc)
