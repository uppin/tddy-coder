# 2026-07-30 — A planned PR repointed onto the default branch starts on it

- The Start-session dialog's "Base branch" picker now pre-selects the planned PR's **derived** base — the same branch its "New branch from base:" caption states — instead of whichever stack branch happened to be listed first; a PR repointed onto `master` no longer starts a child based on an unrelated stack branch, silently undoing the repoint.
- The project's default branch is now offered as a base-branch option (listed last), so a node repointed onto it can show that base and re-pick it; a legacy project storing no default branch offers it as the empty ref labelled *"project default"*.
- Unchanged: a node with a materialized predecessor still pre-selects that dependency, and a root node with no other materialized stack branches still hides the picker and lets the daemon resolve the default base.
