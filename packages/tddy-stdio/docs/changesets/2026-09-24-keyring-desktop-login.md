# 2026-09-24 — `from_duplex` takes the transport its opener names

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`StdioEndpoint::from_duplex(reader, writer, service, transport)`: the endpoint cannot tell a socket from a pipe, so whoever opened the channel names it, and every request hosted over it is stamped with it. `from_process_stdio` and `from_child_stdio` stamp `Pipe`. Pinned by `tests/stamps_the_transport_its_opener_names.rs`. Detail: [stdio-endpoint.md](../stdio-endpoint.md).
