# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`tddy-askpass`, ssh's `SSH_ASKPASS` helper for a daemon-run clone: relays only a key passphrase question to the daemon's loopback endpoint, prints the answer on stdout, and reports a rejected earlier passphrase on stderr when ssh re-asks for the same key. `prompt_text::classify` is the one place that decides which of ssh's questions are key passphrase requests; the daemon uses it too. See [the README](../../README.md).
