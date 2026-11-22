# tddy-test-hosts

The one `ToolHost` a test declares: an environment and a `PATH` it names, over the real filesystem.

Every suite that resolves a coding agent or a declared tool needs a host whose answers it controls, so
nothing of the developer's own `PATH`, `$HOME` or `TDDY_*_BINARY` variables reaches resolution, while the
binaries it resolves are real files the test wrote. That host used to be written out once per suite
(`AFixtureHost`, `APathHost`, `AHostOfStubs`, `AHostOfBinaries`, …); this crate is the one copy.

## Quick Start

```rust
use tddy_test_hosts::a_tool_host;

let host = a_tool_host()
    .with_env("TDDY_CLAUDE_BINARY", stub.display().to_string())
    .with_path([bin_dir])
    .build();
```

### Build and test

```bash
cargo build -p tddy-test-hosts
./test -p tddy-test-hosts
```

## Surface

| Item | Does |
|---|---|
| `a_tool_host()` | an empty `AToolHostBuilder`: no environment variable is set, `PATH` is empty |
| `with_env(name, value)`, `with_envs(vars)`, `with_home(dir)` | declare environment variables; an empty value reads as unset, as `ToolHost::env_var` reads one |
| `with_path(dirs)` | append directories to `PATH`, in search order |
| `build()` | the `AToolHost`: declared environment and `PATH`, real filesystem (a path is executable when it is a regular file with an execute bit; canonicalized by the OS) |

## Why a crate of its own

`tddy-session-lifecycle::test_util` is a public module of a production library, so whatever it depends on
is in the production graph. `tddy-actions` is already there; `tddy-testing-commons` (which pulls in the
sandbox crates and the runner) is not. This crate depends on `tddy-actions` only and is a dev-dependency
of `tddy-agent-launch` and a dependency of `tddy-session-lifecycle`.

What it does not replace: `ToolHost` copies in crates *below* it (`tddy-actions`' own tests would close a
cycle) and fixtures that answer more than a declared environment — see
[the todo](../../docs/dev/todo/2026-11-21-tool-host-fixtures-repeat-across-the-tab-jail-tests.md).

Unix only: `is_executable_file` reads the execute bit.
