# Development TODO — moved

The development TODO is now a **directory of one file per item**:
[`docs/dev/todo/`](todo/).

It was a single 2,100-line document, which made it a shared append-point that conflicted whenever two
branches recorded a finding in the same merge window. The new layout matches
[`docs/dev/changesets/`](changesets/) and the product changelogs — see
[changelog-merge-hygiene.md](guides/changelog-merge-hygiene.md).

There is no index; the directory listing is one, and the date prefix sorts it:

```bash
ls docs/dev/todo/ | sort -r | head -20
grep -rl '<module or topic>' docs/dev/todo/
```

This stub exists only so links in already-wrapped changesets keep resolving. Nothing routine edits
it, and new items are **never** added here — add a file to [`docs/dev/todo/`](todo/) instead.
