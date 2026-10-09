//! What the moved declaration says about who may reach the module, written for its new parent.
//!
//! The same reading `move_item` gives an item: a keyword is a spelling, so it is read as the module
//! subtree it covers, widened only as far as a caller needs, and spelled again for the module it
//! lands in.

use super::super::item_move::scope::Scope;
use super::super::item_move::sites::{module_of_file, Site};
use super::super::item_move::text::enclosing_modules;
use super::super::seam_refusal;
use super::reading::Reparent;
use super::survey::Survey;
use crate::edit::VisibilityChange;
use crate::plan::Reexport;
use crate::registry::Workspace;
use crate::Result;

/// How the declaration is written in its new parent.
pub(super) struct Landing {
    /// The visibility the module had, as the subtree it covered in the old parent.
    pub(super) written: Scope,
    /// The new visibility, as it is spelled in the new parent. Empty for private.
    pub(super) spelled: String,
    /// What the move widened, for the report.
    pub(super) report: Vec<VisibilityChange>,
}

/// The visibility the module needs where it lands.
///
/// With callers re-pointed it is as wide as the callers outside the moved code need; with a facade
/// the callers stay where they are, and the facade in the old parent is the one that must reach it.
pub(super) fn landing(
    workspace: &Workspace<'_>,
    request: &Reparent,
    survey: &Survey,
    sites: &[Site],
    reexport: Reexport,
) -> Result<Landing> {
    let (old, new) = (&request.parent, &survey.new_parent.path);
    let visibility = survey.declaration.visibility.as_str();
    let written = Scope::parse(visibility, old).ok_or_else(|| {
        seam_refusal(format!(
            "the visibility `{visibility}` of `mod {}` is not one this move can read",
            request.name
        ))
    })?;
    let starts_as = if written == Scope::Within(old.clone()) {
        Scope::Within(new.clone())
    } else {
        written.clone()
    };
    let mut scope = starts_as.clone().widened_to(new);
    scope = if reexport.repoints_callers() {
        match users_of(workspace, request, survey, sites)? {
            Some(users) => users
                .iter()
                .fold(scope, |scope, user| scope.widened_to(user)),
            None => Scope::Public,
        }
    } else {
        scope.widened_to(old)
    };

    let spelled = scope.spelled_in(new);
    let report = (scope != starts_as).then(|| VisibilityChange {
        item: request.name.clone(),
        from: as_written(visibility),
        to: as_written(&spelled),
        reason: None,
    });
    Ok(Landing {
        written,
        spelled,
        report: report.into_iter().collect(),
    })
}

/// A visibility as a report says it: `private` for none.
fn as_written(visibility: &str) -> String {
    if visibility.is_empty() {
        "private".to_string()
    } else {
        visibility.to_string()
    }
}

/// The modules that name the module from outside the moved code, or `None` when a file outside the
/// crate does, which only a `pub` module can have.
fn users_of(
    workspace: &Workspace<'_>,
    request: &Reparent,
    survey: &Survey,
    sites: &[Site],
) -> Result<Option<Vec<Vec<String>>>> {
    let mut users = Vec::new();
    for site in sites {
        if survey.files.iter().any(|file| file.from == site.path) {
            continue;
        }
        let Some(base) = module_of_file(
            workspace.root,
            &request.named.package.crate_name,
            &site.path,
        ) else {
            return Ok(None);
        };
        let text = workspace.read(&site.path)?;
        users.push(
            base.into_iter()
                .chain(enclosing_modules(&text, site.offset))
                .collect(),
        );
    }
    Ok(Some(users))
}
