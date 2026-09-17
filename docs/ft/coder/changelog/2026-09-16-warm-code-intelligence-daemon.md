# 2026-09-16 — Warm code-intelligence daemon

`tddy-tools restructure` can run against a warm rust-analyzer index instead of loading the crate
graph on every invocation — six to ten minutes on this workspace, once per retry of an iterative
carve. Start one daemon per checkout and point the CLI at it:

```bash
eval $(./run-index-daemon | grep '^export ')
tddy-tools restructure check plan.jsonl
```

A second request on a warm root is ~2,500× faster than its cold load. With `TDDY_INDEX_SOCKET` unset
the CLI behaves exactly as before; a socket that is set but unreachable is an error rather than a
silent fall back. The daemon reports per request whether the root it named was already warm, so the
difference is visible rather than inferred.

`tddy-index-daemon` serves eleven operations — the restructuring five plus coverage, CRAP reporting,
duplicate-test detection and per-function complexity — over gRPC and stdio, holding one index per
worktree. The same binary runs a single operation and exits when given no transport argument.

**`--indexing-budget` is withdrawn.** A run now waits until it succeeds or its caller stops it, so
there is no budget to state. The flag derived a per-operation ceiling of a twentieth of itself, which
refused large files at 45 seconds regardless of what was asked for.

See [warm-code-intelligence-daemon.md](../warm-code-intelligence-daemon.md).
