//! An agent notification's markdown body, rendered for Telegram's HTML parse mode and for a
//! surface that shows plain text.
//!
//! Feature doc `docs/ft/daemon/session-notifications.md` § Agent-written notifications (`#home-inbox` 1/8). Telegram
//! accepts `b`, `i`, `code`, `pre` and `a` and refuses a whole message over anything else, so every
//! other construct is flattened to text and every stray `<`, `>` and `&` is escaped.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_daemon_kernel::notification_markdown::{to_plain_text, to_telegram_html};

// ---------------------------------------------------------------------------
// to_telegram_html
// ---------------------------------------------------------------------------

#[rstest]
#[case::bold("4 done, **1 blocked**", "4 done, <b>1 blocked</b>")]
#[case::italic("it is *probably* fine", "it is <i>probably</i> fine")]
#[case::inline_code("run `cargo test`", "run <code>cargo test</code>")]
#[case::code_block("```\nlet x = 1;\n```", "<pre>let x = 1;</pre>")]
#[case::link(
    "see [the PR](https://example.com/pr/766)",
    "see <a href=\"https://example.com/pr/766\">the PR</a>"
)]
#[case::nested_bold_and_italic("**all *green***", "<b>all <i>green</i></b>")]
fn the_markup_telegram_accepts_renders_as_its_html_tags(#[case] md: &str, #[case] html: &str) {
    assert_eq!(to_telegram_html(md), html);
}

#[test]
fn paragraphs_are_separated_by_a_blank_line_and_nothing_trails_the_last() {
    assert_eq!(to_telegram_html("first\n\nsecond\n"), "first\n\nsecond");
}

#[rstest]
#[case::an_ampersand("Tom & Jerry", "Tom &amp; Jerry")]
#[case::angle_brackets_in_text("1 < 2 > 0", "1 &lt; 2 &gt; 0")]
#[case::an_inline_script(
    "before <script>alert(1)</script> after",
    "before &lt;script&gt;alert(1)&lt;/script&gt; after"
)]
#[case::an_html_block("<div>raw</div>", "&lt;div&gt;raw&lt;/div&gt;")]
#[case::a_tag_telegram_would_accept_written_as_raw_html("<b>raw</b>", "&lt;b&gt;raw&lt;/b&gt;")]
#[case::markup_inside_code("`a < b && c`", "<code>a &lt; b &amp;&amp; c</code>")]
fn everything_else_that_looks_like_html_is_escaped(#[case] md: &str, #[case] html: &str) {
    assert_eq!(to_telegram_html(md), html);
}

#[test]
fn an_ampersand_in_a_link_target_is_escaped_inside_the_attribute() {
    assert_eq!(
        to_telegram_html("[q](https://example.com/?a=1&b=2)"),
        "<a href=\"https://example.com/?a=1&amp;b=2\">q</a>"
    );
}

#[test]
fn nested_lists_flatten_to_bulleted_lines_indented_by_depth() {
    // Given a list Telegram has no tag for, with a nested level
    let md = "- one\n  - two\n- three";

    // Then each item is its own line, its depth kept as indentation
    assert_eq!(to_telegram_html(md), "• one\n  • two\n• three");
}

#[test]
fn a_heading_flattens_to_bold_text() {
    assert_eq!(
        to_telegram_html("# Results\n\nall green"),
        "<b>Results</b>\n\nall green"
    );
}

// ---------------------------------------------------------------------------
// to_plain_text
// ---------------------------------------------------------------------------

#[test]
fn plain_text_strips_every_kind_of_markup() {
    assert_eq!(
        to_plain_text(
            "**4 done**, *one* `blocked` — [see](https://example.com)",
            200
        ),
        "4 done, one blocked — see"
    );
}

#[test]
fn plain_text_puts_each_paragraph_on_its_own_line() {
    assert_eq!(to_plain_text("first\n\nsecond", 200), "first\nsecond");
}

#[test]
fn plain_text_that_fits_is_returned_whole() {
    assert_eq!(to_plain_text("all green", 9), "all green");
}

#[test]
fn plain_text_over_the_limit_is_cut_to_it_and_ends_in_an_ellipsis() {
    // The ellipsis counts toward the limit: the result is never longer than asked for.
    assert_eq!(to_plain_text("abcdefghij", 5), "abcd…");
}

#[test]
fn a_cut_never_splits_a_multibyte_character() {
    // Given a body of two-byte characters, cut at a count a byte-wise cut would land inside
    let cut = to_plain_text("éééééé", 4);

    // Then whole characters survive, three of them and the ellipsis
    assert_eq!(cut, "ééé…");
}
