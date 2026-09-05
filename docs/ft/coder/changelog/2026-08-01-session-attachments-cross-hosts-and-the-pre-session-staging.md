# 2026-08-01 — Session attachments cross hosts, and the pre-session staging area no longer outlives a restart

- A staged attachment can now be **consumed by a session on another host**: the session's host fetches the bytes from the host that holds them, so uploading to whichever daemon a client is connected to no longer constrains where the session runs. Previously this was refused outright.
- The guarantee that replaced that restriction: a staged file is usable only once its upload is **complete**, checked on the host that owns the bytes — so a cross-host fetch cannot produce a truncated attachment that looks whole.
- The pre-session staging area moved to a directory the host **clears on restart**, so a Start-Session form that is filled in and abandoned no longer leaks its uploads indefinitely.
- `StartSession` and `ReadHostDocument` gained **streaming variants**. The streaming document read carries files past the 4 MiB limit of the unary one, and the streaming session start reports materialization progress per attachment.
- A daemon can be configured with a maximum attachment size, and advertises it, so a client can refuse an oversized file before uploading it rather than after.
- **Fixed a silent hang in daemon-to-daemon RPC forwarding.** A daemon addressed its peers at an identity that serves no RPC, and the forward had no deadline — so any forwarded call waited forever without ever reporting an error. Every test in the repository had stood its fixture peer up at that same identity, which is why it went unnoticed.
- **Fixed a silent 4 MiB ceiling on cross-host attachments.** The same file attached fine on one host and failed across two.
- Listing a worktree directory now reports each file's size, so a client can check a file against the attachment limit before referencing it.
