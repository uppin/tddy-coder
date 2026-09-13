# 2026-09-10 — Endpoint-only daemon

**Type:** Architecture

Removed `connection_service` and the 931-line connection-service doc. Daemon assembles sibling
services; multi-service local socket. See [daemon-endpoint.md](../daemon-endpoint.md).
