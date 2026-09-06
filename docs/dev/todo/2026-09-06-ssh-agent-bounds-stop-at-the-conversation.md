# 2026-09-06 — `ssh_agent.rs`'s bounds stop at the conversation

**Category:** Host tooling probe
**Source:** `#hosts-screen` 5/8, PR #457

- **No cap on the announced answer length.** `ssh_agent_lib`'s `Client::handle` resizes its buffer to
  the length the peer announced before this module's transport sees a byte, so a socket that is not an
  agent can make the daemon reserve up to 4 GiB before the read fails. Mitigated only by reachability
  — `WellKnownAgentSockets` hands back a socket owned by the user being asked about — not by a bound.
- **`getpwnam_r` and `UnixStream::connect` are unbounded.** Both run before any deadline exists and
  neither takes one, so a wedged socket or NSS backend parks the probe thread indefinitely. The Hosts
  screen polls every host it lists, so that is one parked thread per poll per host.
- **One undecodable identity fails the whole list** (`Identity::decode_vec` is all-or-nothing), and
  **a comment that is not UTF-8 sinks the list with it** — a real behavioural regression from the
  hand-rolled reader, which rendered such a comment lossily and kept the key. Reported as a failure
  and never as an empty list, so it cannot read as "this host has no keys loaded"; the host's other
  keys are not listed either.
