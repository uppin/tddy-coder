# 2026-10-22 — The first sandboxed-start suite that passes on a macOS host

**Type:** Test

`sandboxed_cli_harness_acceptance` installs the sandbox RPC bridge (`install_sandbox_rpc_bridge`),
which the older sandboxed suites never do. It then runs a real jail: OpenCode on a Meta account (the
route's model, config file and key), Muse Code on its model and `META_API_KEY`, and a Muse resume that
relaunches the jail with `--continue`. Every jail is terminated on drop. In `opencode_cli_session`,
the two sandbox tests now pin the provider-account refusal. PR
[#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
