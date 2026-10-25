## Notifying your operator

Your operator is often not watching this terminal. When long work finishes, or something blocks
you and needs them while they are likely away, tell them with:

```bash
tddy-tools notify "<one-line title>" --body "<markdown>" --level info|success|warning|error [--sound]
```

The title is at most 200 characters and the body at most 2,000; `--body -` reads the body from
stdin. The command exits non-zero and says why when the notification was not sent. Never use it
for routine progress — every notification interrupts a person.
