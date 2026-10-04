use super::{failure, seam_refusal, Assist};
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The title of the assist `variant` selects for a function returning `returned`, or why not.
    fn the_assist_for(
        variant: &str,
        returned: Option<&str>,
    ) -> std::result::Result<String, String> {
        return_type_assist(variant, returned)
            .map(|assist| assist.title.to_string())
            .map_err(|refusal| refusal.to_string())
    }

    #[test]
    fn unwrap_picks_the_assist_for_what_the_function_returns_now() {
        // Given functions returning a `Result`, an `Option` and a bare `u32`

        // When each is unwrapped
        let titles = [
            the_assist_for("unwrap", Some("Result<u32, String>")),
            the_assist_for("unwrap", Some("std::option::Option<u32>")),
            the_assist_for("unwrap", Some("u32")),
        ];

        // Then the first two get their own assist, and the third is refused saying why
        assert_eq!(
            titles.clone().map(|title| title.ok()),
            [
                Some("unwrap result return type".to_string()),
                Some("unwrap option return type".to_string()),
                None,
            ]
        );
        let [_, _, bare] = titles;
        let reason = bare.expect_err("a bare `u32` has nothing to unwrap");
        assert!(
            reason.contains("`unwrap` needs a function returning a `Result` or an `Option`"),
            "the refusal does not say what `unwrap` needs:\n{reason}"
        );
    }

    #[test]
    fn a_variant_that_is_not_one_of_the_three_is_refused_naming_it() {
        // Given a function returning `u32`

        // When its return type is changed with the variant `box`
        let refusal = the_assist_for("box", Some("u32"));

        // Then the variant is named in the refusal
        assert_eq!(
            refusal,
            Err("plan is malformed: `box` is not a `change_return_type` variant".to_string())
        );
    }

    #[test]
    fn wrapping_does_not_depend_on_what_the_function_returns_now() {
        // Given a function returning `u32`

        // When it is wrapped in a `Result` and in an `Option`
        let titles =
            ["wrap_result", "wrap_option"].map(|variant| the_assist_for(variant, Some("u32")));

        // Then each selects its own assist
        assert_eq!(
            titles,
            [
                Ok("wrap return type in result".to_string()),
                Ok("wrap return type in option".to_string()),
            ]
        );
    }
}
