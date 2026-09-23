//! The worktree's `HEAD`, read from the filesystem rather than by spawning git.
//!
//! Lives in `tddy-core` because both sides of a session need it and neither can reach the other's
//! crate: the daemon stamps records for claude-cli and sandbox sessions, the coder's presenter
//! stamps them for tool and cursor-cli ones, and `tddy-core` is the only crate both already depend
//! on. It is pure filesystem work with no daemon concept in it, so the move costs nothing.
//!
//! Product contract: `docs/ft/daemon/session-worktree-sync.md` AC1.

use std::path::{Path, PathBuf};

/// The worktree's `HEAD` commit, read **from the filesystem** rather than by spawning git.
///
/// A record is stamped with this on every tool call, and an agent makes a great many of them, so
/// the cost has to be a couple of file reads rather than a process. That is the whole reason this
/// exists beside `snapshot_worktree`'s `rev-parse`: the poll loop can afford a subprocess every two
/// seconds, a `Read` tool call cannot.
///
/// Resolves the three shapes a checkout's HEAD takes: a detached sha, a symbolic ref pointing at a
/// loose ref file, and a symbolic ref that only `packed-refs` knows. A session worktree is a linked
/// `git worktree`, so `.git` is a *file* naming the real gitdir, and `packed-refs` lives in the
/// **common** dir rather than beside HEAD.
///
/// Returns an empty string when HEAD cannot be resolved — an unborn branch, an unreadable path,
/// anything. Empty is honest; AC1 forbids fabricating a sha, because a mirror that trusts one would
/// be confidently wrong rather than merely uninformed.
/// `worktree_root` is the root of a checkout and nothing else: unlike `git`, this does not walk up
/// looking for a repository, because every caller already holds the root it means and a search would
/// answer for some enclosing repository instead of saying it does not know.
pub fn read_head_commit(worktree_root: &Path) -> String {
    let Some(git_dir) = git_dir_of(worktree_root) else {
        return String::new();
    };
    // Where the shared refs live. For an ordinary checkout that is the git dir itself; for a linked
    // worktree it is the main repository's, which is the only place `refs/heads/*` and `packed-refs`
    // exist — a linked worktree has its own HEAD and shares everything HEAD points at.
    let common_dir = common_dir_of(&git_dir);

    let mut name = String::from("HEAD");
    // Bounded rather than looped until it resolves: a symbolic ref may point at another symbolic
    // ref, and a repository whose refs form a cycle — corrupt, or written by something that is not
    // git — must cost this a handful of file reads and not a hung tool call.
    for _ in 0..MAX_SYMBOLIC_REF_HOPS {
        let Some(contents) = read_ref(&git_dir, &common_dir, &name) else {
            // An unborn branch lands here: HEAD names `refs/heads/main`, and no such ref exists
            // until the first commit creates it.
            return String::new();
        };
        match contents.strip_prefix(SYMBOLIC_REF_PREFIX) {
            Some(target) => name = target.trim().to_string(),
            None if is_object_id(&contents) => return contents,
            // Neither a symbolic ref nor an object id. Returning it would hand a caller whatever
            // bytes happened to be in the file as if it were a commit, which is the fabrication AC1
            // forbids in its least obvious form.
            None => {
                log::warn!(
                    "session_room: {name} of {worktree_root:?} is neither a ref nor an object id"
                );
                return String::new();
            }
        }
    }
    log::warn!(
        "session_room: HEAD of {worktree_root:?} did not resolve in {MAX_SYMBOLIC_REF_HOPS} hops"
    );
    String::new()
}

/// How many symbolic refs deep HEAD may be before this gives up. Git's own limit for the same
/// traversal is five, and a real repository uses one.
const MAX_SYMBOLIC_REF_HOPS: usize = 5;

/// What a ref file holding a symbolic ref begins with: `ref: refs/heads/main`.
const SYMBOLIC_REF_PREFIX: &str = "ref:";

/// The git directory `worktree_root` keeps its HEAD in.
///
/// `.git` is a directory for an ordinary checkout and a *file* for a linked `git worktree`, which is
/// what every session worktree is — so the file case is the common path here, not the exotic one.
/// That file holds `gitdir: <path>`, absolute or relative to the checkout.
fn git_dir_of(worktree_root: &Path) -> Option<PathBuf> {
    let dot_git = worktree_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let named = pointer.trim().strip_prefix("gitdir:")?.trim();
    if named.is_empty() {
        return None;
    }
    Some(worktree_root.join(named))
}

/// The directory a linked worktree shares its refs with, named by `<git_dir>/commondir` — usually
/// relative to the git dir. An ordinary checkout has no such file and is its own common dir.
fn common_dir_of(git_dir: &Path) -> PathBuf {
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(named) if !named.trim().is_empty() => git_dir.join(named.trim()),
        _ => git_dir.to_path_buf(),
    }
}

/// The contents of one ref: a loose file, or the line `packed-refs` holds for it.
///
/// The git dir is tried before the common dir because that ordering is what makes both kinds of ref
/// resolve through one lookup. Per-worktree refs — HEAD itself, `refs/bisect/*`, `refs/worktree/*` —
/// exist only in the git dir, and a linked worktree's shared refs exist only in the common dir; for
/// an ordinary checkout the two are the same directory and the second read never happens.
fn read_ref(git_dir: &Path, common_dir: &Path, name: &str) -> Option<String> {
    if !is_plausible_ref_name(name) {
        return None;
    }
    if let Ok(loose) = std::fs::read_to_string(git_dir.join(name)) {
        return Some(loose.trim().to_string());
    }
    if let Ok(loose) = std::fs::read_to_string(common_dir.join(name)) {
        return Some(loose.trim().to_string());
    }
    // Packed last, and not skippable: `git gc` packs refs away routinely, so a reader that only
    // knew loose files would answer "" for every repository that has been collected once — which
    // is every long-lived one.
    packed_ref(common_dir, name)
}

/// The object id `packed-refs` records for `name`.
///
/// Each line is `<oid> <refname>`, with a `#` header and `^<oid>` lines carrying the commit a tag
/// peels to. The peel line belongs to the ref above it and names no ref of its own, so taking it for
/// one would answer some other ref's lookup with a tag's target.
fn packed_ref(common_dir: &Path, name: &str) -> Option<String> {
    let packed = std::fs::read_to_string(common_dir.join("packed-refs")).ok()?;
    packed.lines().find_map(|line| {
        let line = line.trim_end();
        if line.starts_with('#') || line.starts_with('^') {
            return None;
        }
        let (oid, packed_name) = line.split_once(' ')?;
        (packed_name == name && is_object_id(oid)).then(|| oid.to_string())
    })
}

/// Whether `name` is a ref name this is willing to open a file for.
///
/// A ref name is a path relative to the git dir, so one carrying `..` or an absolute root would read
/// a file outside the repository entirely. Git forbids both in `check-ref-format`; this refuses them
/// because the name comes out of a file on disk, and a checkout is not a source this trusts to have
/// been written by git.
fn is_plausible_ref_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && !name
            .split('/')
            .any(|part| part == "." || part == ".." || part.is_empty())
}

/// Whether `candidate` has the shape of an object id — 40 hex digits under SHA-1, 64 under SHA-256,
/// the only two object formats git has.
///
/// Shape only. Whether the object exists is a question that needs the object database, which is the
/// subprocess this whole function exists to avoid.
fn is_object_id(candidate: &str) -> bool {
    matches!(candidate.len(), 40 | 64) && candidate.bytes().all(|b| b.is_ascii_hexdigit())
}
