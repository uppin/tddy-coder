# 2026-03-09 — gRPC Remote Control

**Type:** Feature

Single `--grpc [PORT]` option (default 50051). Creates broadcast + mpsc channels, Presenter with with_broadcast, spawns tonic gRPC server in thread, passes intent_rx to run_event_loop. Dependencies: tonic, tokio-stream. (tddy-coder)
