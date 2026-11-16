# tddy-livekit-compat-tests

LiveKit **connectivity and contract** suites. Every test here proves that a LiveKit adapter
connects to a real LiveKit server and honours the contract its cheap adapters honour in the
crates that own the behaviour: same RPC envelopes, metadata, streaming, rejoin and deadline
semantics.

No product logic lives here. A suite that asserts a use-case belongs in the crate that owns the
use-case, running over an in-memory, stdio or local IPC transport with no container and no
network. `tests/livekit_test_placement.rs` pins the split and names the logic suites still
converting.

Run them against a reusable server:

```bash
eval $(./run-livekit-testkit-server | grep '^export ')
./test -p tddy-livekit-compat-tests -p tddy-livekit-testkit
```

Without `LIVEKIT_TESTKIT_WS_URL`, each suite starts its own container. On a PR, the
`Rust LiveKit e2e tests` check runs exactly this package (plus the testkit's own suites) against
one shared server, selected by `.config/rust-livekit-e2e.filterset`.
