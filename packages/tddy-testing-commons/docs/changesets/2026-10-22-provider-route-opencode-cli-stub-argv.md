# 2026-10-22 — A no-argument stub invocation is an invocation

**Type:** Fix

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `read_last_appended_argv` reads an empty last line (a spawn with no arguments) as no arguments,
not as no invocation, and `appended_invocation_count` counts it.
