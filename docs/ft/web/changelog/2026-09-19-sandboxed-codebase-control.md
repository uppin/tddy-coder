# 2026-09-19 — Sandboxed codebase placement in the new-session form

- **A `Sandboxed codebase` checkbox** on a claude-cli session asks the daemon to jail the checkout
  and run the agent beside it. It rides through as `sandboxedCodebase` on `StartSession`.
- **Three controls now name a placement and a session has one.** Choosing `Sandboxed codebase`
  clears `Sandbox` and `Managed codebase` **in the form**, and choosing either of those clears it —
  visibly, not silently at submit. `Sandbox` and `Managed codebase` remain combinable with each
  other, which is what a split placement uses.
- **`Dangerously skip permissions` is withdrawn** on this placement, as it is on a split: the
  confinement rests on a deny list whose survival under that flag this repo does not pin.
- **A host that does not advertise the capability gets a disabled control naming it**, not a hidden
  one — read from the daemon's own self-description, so a deployment with no LiveKit room is served.
- **A host whose jail shares the filesystem root says so**, beside an enabled control: it confines
  process and network, not writes outside the checkout.
