# 2026-10-06 — A waiting restructure run says what it is waiting for

Feature: [Rust code restructuring](../rust-code-restructuring.md).

A restructure run that waits for rust-analyzer can wait for many minutes, and by design nothing but the
caller ends the wait. Once the server stopped sending new progress, the run said nothing at all. Every
polling wait now prints, on a fixed 30-second cadence (`WAIT_HEARTBEAT`), the stage it is in, how long
it has waited, which server it is waiting on, the server's last words and how long they have been
unchanged; a wait ended by cancellation names its stage. **It adds no deadline** — a run still waits
until the server is ready or its caller stops it.

- **What a beat says.** `still waiting (<elapsed>) — <stage>: rust-analyzer (pid N)` (a server the run
  started) or `rust-analyzer behind a shared client`, then whether it says it is still loading, its last
  words with how long they have been unchanged, and the furthest phase it reported. The `unchanged for`
  figure is the one that separates a slow server from a stuck one.
- **Every wait, and a request in flight.** The five polling waits — the warm-up, type inference at an
  anchor, an assist's answer, an outline, a symbol — each name their own stage; a request already sent
  to the shared client is narrated while it waits, not only the polling between requests.
- **The daemon's queue.** A run queued behind another operation on the same root says so and for how
  long; the next failed send notices a caller that has hung up within one cadence, releasing the root a
  silent, abandoned run would otherwise hold.
- **It diagnoses, it does not cure.** The heartbeat reads readiness and the clock and writes neither; a
  test pins that a cancelled wait leaves the root not ready. rust-analyzer's own children (a build
  script) stay invisible — the quoted progress text is evidence, not identification. The `warm` stream
  is not narrated (a recorded follow-up), and a cold, self-started server is narrated between requests
  but not during one.
