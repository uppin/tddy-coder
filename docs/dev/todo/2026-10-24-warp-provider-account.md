# 2026-10-24 — Warp runs only on the person's own Warp sign-in

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Decisions & Trade-offs (native Warp login only)

`Harness::WarpCli` refuses every provider preset (`HarnessCannotBeRouted`), the web offers Warp no
Provider field, and there is no `warp` preset in `tddy-accounts`. A Warp session uses whatever
`warp` login the daemon's user already has; a headless host with no login cannot run one.

**Why deferred:** connecting an account probes an HTTP model endpoint
(`provider_model_probe.rs`), and a Warp API key has none; a preset needs a probe that fits it.

**What would close it:** a `warp` preset whose secret is handed to the child as `WARP_API_KEY`, a
probe (or an explicit no-probe) for it, `route_supported(WarpCli, "warp")` accepting it, and the
Accounts screen offering it.
