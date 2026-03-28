//! Resolve `PRD.md` for plan review when artifacts may be written under a nested directory
//! (`.../sessions/<uuid>/<dated-feature-slug>/`) while the canonical PRD lives at
//! `.../sessions/<uuid>/PRD.md`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Prefer `.../sessions/<uuid>/PRD.md` when `session_dir` is under that UUID directory and the
/// file exists; otherwise [`Path::join`] `"PRD.md"` on `session_dir`.
pub fn plan_prd_path_for_session_dir(session_dir: &Path) -> PathBuf {
    prd_at_sessions_uuid_root(session_dir).unwrap_or_else(|| session_dir.join("PRD.md"))
}

fn prd_at_sessions_uuid_root(session_dir: &Path) -> Option<PathBuf> {
    let mut current = session_dir.to_path_buf();
    loop {
        let parent = current.parent()?;
        if parent.file_name() == Some(OsStr::new("sessions")) {
            let prd = current.join("PRD.md");
            if prd.is_file() {
                return Some(prd);
            }
            return None;
        }
        if parent == current {
            break;
        }
        current = parent.to_path_buf();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_under_sessions_uuid_prefers_uuid_root_prd() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-nested-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let uuid = root
            .join("sessions")
            .join("a97addd3-c31b-442b-a6b0-a63abe99e11d");
        let nested = uuid.join("2026-03-24-feature");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(uuid.join("PRD.md"), "FULL\n").unwrap();
        std::fs::write(nested.join("PRD.md"), "legacy-nested-only\n").unwrap();

        let path = plan_prd_path_for_session_dir(&nested);
        assert_eq!(path, uuid.join("PRD.md"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "FULL\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn session_dir_at_uuid_uses_that_prd() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-at-uuid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let uuid = root.join("sessions").join("uuid-here");
        std::fs::create_dir_all(&uuid).unwrap();
        std::fs::write(uuid.join("PRD.md"), "at-uuid\n").unwrap();

        let path = plan_prd_path_for_session_dir(&uuid);
        assert_eq!(path, uuid.join("PRD.md"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_without_sessions_segment_uses_direct_join() {
        let root =
            std::env::temp_dir().join(format!("tddy-session-prd-flat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("PRD.md"), "only\n").unwrap();
        assert_eq!(plan_prd_path_for_session_dir(&root), root.join("PRD.md"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
