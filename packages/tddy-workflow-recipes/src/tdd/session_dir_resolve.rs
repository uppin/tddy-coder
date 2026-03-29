//! Resolve the filesystem session directory for plan — must already exist (no allocation here).

use std::path::PathBuf;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::workflow::context::Context;

/// Returns the session directory for plan from context. The directory must already exist.
///
/// Callers (CLI, daemon, RPC, or tests mimicking them) must create `{base}/sessions/<id>/`
/// before running plan.
pub fn resolve_existing_session_dir_for_plan(context: &Context) -> Result<PathBuf, String> {
    if let Some(p) = context.get_sync::<PathBuf>("session_dir") {
        if p.is_dir() {
            return Ok(p);
        }
        return Err(format!("session_dir {:?} is not an existing directory", p));
    }
    if let (Some(base), Some(sid)) = (
        context.get_sync::<PathBuf>("session_base"),
        context.get_sync::<String>("session_id"),
    ) {
        let trimmed = sid.trim();
        if trimmed.is_empty() {
            return Err(
                "session_id is empty; cannot resolve session directory under session_base"
                    .to_string(),
            );
        }
        let expected = base.join(SESSIONS_SUBDIR).join(trimmed);
        if expected.is_dir() {
            return Ok(expected);
        }
        return Err(format!(
            "expected session directory {:?} to exist (create {}/{}/{{session_id}}/ before running plan)",
            expected,
            base.display(),
            SESSIONS_SUBDIR
        ));
    }
    Err(
        "plan requires session_dir in context, or session_base plus session_id with an existing \
         {session_base}/sessions/{session_id}/ directory (entry layer must create the tree)"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::workflow::context::Context;

    #[test]
    fn resolves_session_dir_when_present_and_is_dir() {
        let tmp = std::env::temp_dir().join(format!("tddy-sdr-session-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let ctx = Context::new();
        ctx.set_sync("session_dir", tmp.clone());
        assert_eq!(resolve_existing_session_dir_for_plan(&ctx).unwrap(), tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolves_from_session_base_and_id_when_dir_exists() {
        let base = std::env::temp_dir().join(format!("tddy-sdr-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sid = "019d357e-48ee-7c11-bd44-a967873f58b2";
        let expected = base.join(SESSIONS_SUBDIR).join(sid);
        std::fs::create_dir_all(&expected).unwrap();
        let ctx = Context::new();
        ctx.set_sync("session_base", base.clone());
        ctx.set_sync("session_id", sid.to_string());
        assert_eq!(
            resolve_existing_session_dir_for_plan(&ctx).unwrap(),
            expected
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn errs_when_session_base_id_path_missing() {
        let base = std::env::temp_dir().join(format!("tddy-sdr-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ctx = Context::new();
        ctx.set_sync("session_base", base.clone());
        ctx.set_sync(
            "session_id",
            "019d357e-48ee-7c11-bd44-a967873f58b2".to_string(),
        );
        assert!(resolve_existing_session_dir_for_plan(&ctx).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
