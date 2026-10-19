//! Rule 2: a module its parent re-exports by glob has every non-private item reached, whether or not
//! a path names it — a trait imported only so its methods can be called is reached by no reference.

use super::{written_visibility, Widened};
use crate::crate_move::manifest_edits::position_of;
use crate::crate_move::moving::parent_reexports_of;
use crate::crate_move::{DeclarationKind, ModuleHome, Result, Survey};
use crate::registry::Workspace;

/// The parent's top-level `use <module>::*;` of `home`, as `<file>:<line>: <statement>`, when the
/// file that declares the module holds one at any visibility.
///
/// A `use <module>::{a, b};` group is not a glob: its leaves are references, which rule 1 reads.
///
/// # Errors
///
/// Refuses when the declaring file cannot be read.
pub(crate) fn reexport_of(workspace: &Workspace<'_>, home: &ModuleHome) -> Result<Option<String>> {
    let Some(module) = home.path.last() else {
        return Ok(None);
    };
    let text = workspace.read(&home.declared_in)?;
    let glob = parent_reexports_of(&text, module)
        .into_iter()
        .find(|at| text[at + module.len() + "::".len()..].starts_with('*'));
    Ok(glob.map(|at| {
        let line = position_of(&text, at).line;
        let statement = text.lines().nth(line as usize - 1).unwrap_or_default();
        format!("{}:{line}: {}", home.declared_in, statement.trim())
    }))
}

/// Every top-level item of the surveyed module not written private, when the survey names a parent
/// glob, each with the reason `` through `<file>`'s glob ``.
///
/// # Errors
///
/// Refuses when the moving file cannot be read, or a declaration's keyword is on a line above its
/// name.
pub(crate) fn glob_visible(workspace: &Workspace<'_>, survey: &Survey) -> Result<Vec<Widened>> {
    let Some((glob_file, _)) = survey
        .reexported_by
        .as_deref()
        .and_then(|glob| glob.split_once(':'))
    else {
        return Ok(Vec::new());
    };
    let text = workspace.read(&survey.source)?;
    let mut visible = Vec::new();
    for item in survey
        .declarations
        .iter()
        .filter(|item| item.kind == DeclarationKind::Item && item.within.is_empty())
    {
        let written = written_visibility(&survey.source, &text, item)?;
        if written.is_some_and(|written| !written.is_empty() && written != "pub") {
            visible.push(Widened::of(
                &survey.source,
                item,
                Some(format!("through `{glob_file}`'s glob")),
            ));
        }
    }
    Ok(visible)
}
