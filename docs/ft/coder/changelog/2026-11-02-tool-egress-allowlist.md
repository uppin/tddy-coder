# 2026-11-02 — A tool declares the hosts it may reach, and the jail enforces it

`#agent-tools` 3/12 ([#792](https://github.com/uppin/tddy-coder/pull/792)).

- **Tool definitions gain `network:`** — `egress: none | any | { hosts: [...] }` and `oauth_inbound`.
  Absent means `none`. A malformed host pattern is refused at load, naming the tool.
- **A jail's policy is the union of its tools'**: `any` dominates, host lists combine, `none` adds
  nothing. A jail of only `none` tools starts no egress shim and sets no proxy environment.
- **An undeclared host is refused twice, independently** — the in-jail shim answers `403` naming the
  host, and the host relay refuses the `TunnelOpen` even when the shim is bypassed. Each refusal is
  logged with the session and the tool set that lacked the host.
- Coding-agent jails keep unrestricted egress until their own nodes declare hosts.

See [Sandbox builder § Tool egress](../sandbox-builder.md#tool-egress).
