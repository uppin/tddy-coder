# 2026-10-24 — Session starts no longer queue behind a clone

On a host without a supervisor (Tddy Desktop, `./install --user`), the daemon's spawn worker serves
several requests at once: a long clone no longer holds every other session start behind it, a request's
answer is never delivered to another caller, and a clone reports its `git` pid before it finishes. If the
worker exits, every pending start fails at once naming the worker's pid. See
[systemd install](../systemd-install.md#spawn-worker-unsupervised-hosts).
