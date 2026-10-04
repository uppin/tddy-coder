use super::failure;

use super::seam_refusal;

use super::Assist;

use crate::Result;

/// The assist behind `change_return_type`'s `variant`, for a function returning `returned`.
///
/// One operation, several assists: `unwrap` is `Unwrap Option return type` or `Unwrap Result return
/// type` depending on what the declaration returns now, so this is keyed by the variant and the
/// current return type rather than the kind, and is not one of [`assist_for`]'s arms. Each rewrites
/// the declaration and the function's own returns, in its own file; callers are left to the group's
/// gate. The titles are the bundled rust-analyzer's own (`wrap_return_type`, `unwrap_return_type`).
pub(crate) fn return_type_assist(variant: &str, returned: Option<&str>) -> Result<Assist> {
    const fn rewriting_the_return_type(title: &'static str) -> Assist {
        Assist {
            title,
            kinds: &["refactor.rewrite"],
            at_caret: true,
            placeholder: None,
            multi_file: false,
            needs_inference: false,
            relocates_items: false,
        }
    }
    let wrapper = || {
        let returned = returned?;
        match syn::parse_str::<syn::Type>(returned).ok()? {
            syn::Type::Path(path) => Some(path.path.segments.last()?.ident.to_string()),
            _ => None,
        }
    };
    match variant {
        "wrap_result" => Ok(rewriting_the_return_type("wrap return type in result")),
        "wrap_option" => Ok(rewriting_the_return_type("wrap return type in option")),
        "unwrap" => match wrapper().as_deref() {
            Some("Result") => Ok(rewriting_the_return_type("unwrap result return type")),
            Some("Option") => Ok(rewriting_the_return_type("unwrap option return type")),
            _ => Err(seam_refusal(
                "`unwrap` needs a function returning a `Result` or an `Option`",
            )),
        },
        other => Err(failure(format!(
            "`{other}` is not a `change_return_type` variant"
        ))),
    }
}
