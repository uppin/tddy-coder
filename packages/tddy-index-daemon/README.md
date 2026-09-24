# tddy-index-daemon

A warm rust-analyzer index, served as `code_index.CodeIndexService` over gRPC and stdio — or one
operation run in process and then exit.

Owns `proto/code_index.proto`, serves it, and publishes its coordinate. Eleven RPCs covering the
plan-driven restructuring operations (`tddy-code-restructuring`) and the analysis operations
(`tddy-code-analysis`), each request naming the `workspace_root` it acts on so one process serves
several worktrees.

```bash
tddy-index-daemon restructure check --workspace-root . plan.jsonl   # run once, exit(0|1)
tddy-index-daemon --grpc-uds /run/tddy/index.sock                   # serve, stay alive
tddy-index-daemon --grpc 127.0.0.1:7777 --stdio                     # both, one warm index
```

A transport argument selects the serving lifetime; its absence selects single-shot. Neither a
subcommand nor a transport is an error rather than a default.

- **Contract, warm-state model and refusal classes**:
  [`docs/code-index-service.md`](docs/code-index-service.md)
- **Product documentation**:
  [`docs/ft/coder/warm-code-intelligence-daemon.md`](../../docs/ft/coder/warm-code-intelligence-daemon.md)
- **Running it**: `./run-index-daemon` at the repo root starts or reuses one per checkout and prints
  `export TDDY_INDEX_SOCKET=…`. It launches the daemon with the dev shell's **whole** environment
  (not only its `PATH`), because rust-analyzer builds every build script and proc macro in it, and
  with a temporary directory that outlives the shell that started it
