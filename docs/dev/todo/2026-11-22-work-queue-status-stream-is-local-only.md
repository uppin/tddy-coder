# `StreamWorkQueueStatus` is served only for a queue this host holds

**Found:** `#workqueue` 6/14 (`/green`)

The unary work-queue RPCs forward an address naming a peer (`forward_addressed`). A server stream does
not: `stream_work_queue_status_rpc` answers `unimplemented` for a queue another host holds, so a view of
a remote queue shows no live check counts (the rest of the row still works).

**Fix:** a peer-forwarded stream in `PeerRouting` (none exists today), or the browser opening the stream
against the daemon that holds the queue.
