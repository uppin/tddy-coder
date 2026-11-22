# 2026-11-21 — A dead main jail ends its terminal

A sandboxed session whose main [jail](../session-liveness.md#a-dead-main-jail) dies on its own now ends its main
terminal (the exit status is recorded), leaves the registry and lists inactive, resumable, with its tab jails still
running. A jail stopped by request is not reported as having died.
