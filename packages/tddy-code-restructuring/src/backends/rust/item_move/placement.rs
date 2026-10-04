//! Where the moved text lands in the destination module.

use std::ops::Range;

use super::super::seam_refusal;
use super::text::Edit;
use crate::crate_move::source_scan::items_of_module;
use crate::Result;

/// The insertion of `moved` into the module spanning `scope` of `text`.
///
/// A file module takes the items at its end, or above its trailing `#[cfg(test)]` module: items
/// after a test module are what `clippy::items_after_test_module` fails the lint gate for. An inline
/// module takes them on the lines before its closing brace, indented to match.
pub(super) fn insertion(text: &str, scope: &Range<usize>, moved: &str) -> Result<Edit> {
    if scope.start == 0 {
        return Ok(match test_module_start(text, scope) {
            Some(at) => Edit::insert(at, format!("{moved}\n")),
            None => {
                let lead = if text.is_empty() || text.ends_with("\n\n") {
                    ""
                } else if text.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                Edit::insert(text.len(), format!("{lead}{moved}"))
            }
        });
    }

    let closing_line = text[..scope.end]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let before_brace = &text[closing_line..scope.end];
    if !before_brace.trim().is_empty() || closing_line < scope.start {
        return Err(seam_refusal(
            "the destination is an inline module written on one line: put its closing brace on a \
             line of its own and plan again",
        ));
    }
    let indent = format!("{before_brace}    ");
    let lead = if text[scope.start..closing_line].trim().is_empty() {
        ""
    } else {
        "\n"
    };
    let indented: String = moved
        .lines()
        .map(|line| {
            if line.is_empty() {
                "\n".to_string()
            } else {
                format!("{indent}{line}\n")
            }
        })
        .collect();
    Ok(Edit::insert(closing_line, format!("{lead}{indented}")))
}

/// What stands where the lines `region` were, now that they have left `text`: `lines` (a facade,
/// each ending a line of its own) or nothing, in which case the blank line that separated the
/// region from what follows goes too, so no double blank line is left behind.
pub(in crate::backends::rust) fn vacated(
    text: &str,
    region: &Range<usize>,
    lines: &[String],
) -> Edit {
    let mut end = region.end;
    let after_blank = region.start == 0 || text[..region.start].ends_with("\n\n");
    if lines.is_empty() && after_blank && text[end..].starts_with('\n') {
        end += 1;
    }
    let replacement: String = lines.iter().map(|line| format!("{line}\n")).collect();
    Edit::replace(region.start..end, replacement)
}

/// Where the first inline `#[cfg(test)]` module of the module begins, its attributes included.
fn test_module_start(text: &str, scope: &Range<usize>) -> Option<usize> {
    let items = items_of_module(&text[scope.clone()]);
    items.children.iter().find_map(|child| {
        let body = child.body.as_ref()?;
        let opened = scope.start + body.start;
        let declared = text[..opened].rfind(&format!("mod {}", child.name))?;
        let mut start = text[..declared]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let mut tests = false;
        while start > 0 {
            let above_end = start - 1;
            let above_start = text[..above_end]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let line = text[above_start..above_end].trim();
            if !line.starts_with("#[") {
                break;
            }
            tests |= line == "#[cfg(test)]";
            start = above_start;
        }
        tests.then_some(start)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(text: &str, moved: &str) -> String {
        let edit = insertion(text, &(0..text.len()), moved).unwrap();
        super::super::text::applied(text, &[edit]).unwrap()
    }

    #[test]
    fn appends_to_the_end_of_a_file_module_after_a_blank_line() {
        assert_eq!(
            placed("fn a() {}\n", "fn b() {}\n"),
            "fn a() {}\n\nfn b() {}\n"
        );
    }

    #[test]
    fn puts_the_items_above_a_trailing_test_module() {
        let text = "fn a() {}\n\n#[cfg(test)]\nmod tests {\n}\n";

        assert_eq!(
            placed(text, "fn b() {}\n"),
            "fn a() {}\n\nfn b() {}\n\n#[cfg(test)]\nmod tests {\n}\n"
        );
    }

    #[test]
    fn indents_the_items_of_an_inline_module() {
        let text = "mod m {\n    fn a() {}\n}\n";
        let scope = text.find('{').unwrap() + 1..text.rfind('}').unwrap();
        let edit = insertion(text, &scope, "fn b() {}\n").unwrap();

        assert_eq!(
            super::super::text::applied(text, &[edit]).unwrap(),
            "mod m {\n    fn a() {}\n\n    fn b() {}\n}\n"
        );
    }
}
