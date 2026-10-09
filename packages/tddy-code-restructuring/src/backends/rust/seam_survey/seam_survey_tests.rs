//! What the seam survey refuses to vouch for, and where a file's own test module lies.

use super::*;

use crate::RestructureError;

fn lines(from: u32, to: u32) -> Range {
    Range {
        start: Position { line: from, col: 1 },
        end: Position { line: to, col: 1 },
    }
}

#[test]
fn a_range_declaring_items_whose_survey_found_none_is_refused_naming_the_range_and_the_first_item()
{
    // Given a range declaring two documented `pub` items, and a survey that found neither
    let range_text = "/// The bytes of one file.\n#[derive(Debug)]\npub struct PreImage {\n    \
                      pub path: String,\n}\n\npub struct OpenGroup;\n";

    // When the range is weighed against that survey
    let refusal = refuse_unsurveyed_range(range_text, "src/journal.rs", lines(10, 16), &[])
        .expect_err("an empty survey over declared items is refused");

    // Then the refusal names the range, its file and the first item it declares
    match refusal {
        RestructureError::SeamRefused(reason) => assert!(
            reason.starts_with(
                "the range 10-16 of src/journal.rs declares `PreImage` (and 1 more), and the \
                 server's outline lists none of them"
            ),
            "{reason}"
        ),
        other => panic!("refused as the wrong class: {other}"),
    }
}

#[test]
fn a_range_holding_only_impl_blocks_is_not_refused_for_an_empty_survey() {
    // Given a range of one inherent `impl`, which no module path names
    let range_text = "impl Gauge {\n    pub fn level(&self) -> u32 {\n        1\n    }\n}\n";

    // When the range is weighed against an empty survey
    let outcome = refuse_unsurveyed_range(range_text, "src/gauge.rs", lines(3, 7), &[]);

    // Then nothing is refused: an `impl` is moved with its type, not through a facade
    assert!(outcome.is_ok(), "{outcome:?}");
}

#[test]
fn the_test_module_lines_of_a_file_cover_its_inline_cfg_test_module_and_not_an_out_of_line_declaration(
) {
    // Given a file with an out-of-line test module declaration and an inline test module
    let text = "pub fn level() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod level_tests;\n\n\
                #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn one() {\n        \
                assert_eq!(level(), 1);\n    }\n}\n";

    // When its test module lines are read
    let spans = test_module_lines(text);

    // Then only the inline module, from its attribute to its closing brace, is covered
    assert_eq!(spans, vec![8..=16]);
}
