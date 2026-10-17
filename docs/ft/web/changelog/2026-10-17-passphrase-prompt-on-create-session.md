# 2026-10-17 — The create-session form asks for a clone's key passphrase

- **The passphrase dialog opens over the create-session form** while a start waits on a clone that needs a
  key passphrase, for the host that is cloning: the codebase host on a split session, the session's own
  host otherwise. It encrypts under the host's pinned key like the Hosts screen's add-key dialog.
- **A form nobody is submitting opens no prompt stream.**
- The prompt, its key-continuity check and its answer are one hook shared with the Hosts screen's add-key
  row (`useHostPromptAnswer`).

See [loading a key into a host's ssh-agent](../hosts-screen-add-key.md#the-same-question-while-a-session-starts).
