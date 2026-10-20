# 2026-10-21 — `PeerRouting::local_only`

`#satellite` 13/13 (#635). Routing for a daemon with no peers and no common room, named the way its
config names it: what `DaemonSessionHost::new` used to build for itself when given no discovery.
