# 2026-11-15 — Rename, soft-delete and restore a project on every host owning it

`UpdateProject`, `DeleteProject` and `RestoreProject` act on every host holding the project. A delete
tombstones the registry row and keeps everything on disk, is refused while one of the project's
sessions runs, and stops the project's main. A deleted project is hidden from listings and takes no
new work; a host that missed a delete or a restore is reconciled by the next listing. Creating a
deleted project's repository again, or adding it to a host holding its tombstone, restores it. See
[Project concept § Renaming, deleting and restoring a project](../project-concept.md#renaming-deleting-and-restoring-a-project).
