# 2026-11-02 — Tool definitions declare their jail: home, files, secrets, policy

`#agent-tools` 5/12 ([#794](https://github.com/uppin/tddy-coder/pull/794)). A `ToolDef` may carry
`home:` (persistent `<tddyhome>/homes/<key>` or per-session), `files:` (seed-once copies, mirrors,
env overlays), `secrets:` (host env or file, optionally guarded `when: unrouted` / `routed`) and
`policy:` (JIT, fork, sysctl reads, exec paths). `tddy-sandbox-runner --exec-plan` runs a
host-rendered argv, env and files without knowing which tool it is. A provider route's base-URL host
is admitted to the jail's egress. See [Sandbox builder § Jail sections](../sandbox-builder.md#jail-sections).
