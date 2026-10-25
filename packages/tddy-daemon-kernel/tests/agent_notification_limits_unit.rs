//! How long an agent notification may be: a title of 1–200 characters after trimming, a body of
//! at most 2,000. Over a limit is a refusal that names what was too long — never a truncation.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-agent-notifications.md` (`#home-inbox` 1/8).

use rstest::rstest;
use tddy_daemon_kernel::agent_notification_limits::validate_agent_notification;

const A_TITLE: &str = "Parallel run finished";
const A_BODY: &str = "4 done, **1 blocked**";

/// `n` copies of `c` — a string exactly `n` characters long.
fn chars(c: char, n: usize) -> String {
    std::iter::repeat_n(c, n).collect()
}

trait LimitVerdictAssertions {
    fn assert_accepted(&self);
    fn assert_refused_naming(&self, part: &str);
}

impl LimitVerdictAssertions for Result<(), String> {
    fn assert_accepted(&self) {
        assert_eq!(
            self,
            &Ok(()),
            "expected the notification to be within its limits"
        );
    }

    fn assert_refused_naming(&self, part: &str) {
        match self {
            // The wording is the implementation's; what is fixed is that it names what to shorten.
            Err(reason) => assert!(
                reason.contains(part),
                "the refusal names the {part}: {reason:?}"
            ),
            Ok(()) => panic!("expected a refusal naming the {part}, it was accepted"),
        }
    }
}

#[rstest]
#[case::one_character(chars('t', 1))]
#[case::exactly_200_characters(chars('t', 200))]
#[case::two_hundred_multibyte_characters(chars('é', 200))]
#[case::one_character_inside_whitespace(" \t t \n".to_string())]
fn a_title_of_1_to_200_characters_after_trimming_is_accepted(#[case] title: String) {
    validate_agent_notification(&title, A_BODY).assert_accepted();
}

#[rstest]
#[case::empty(String::new())]
#[case::only_whitespace(" \t \n".to_string())]
#[case::two_hundred_and_one_characters(chars('t', 201))]
#[case::two_hundred_and_one_characters_once_trimmed(format!("  {}  ", chars('t', 201)))]
fn a_title_empty_after_trimming_or_over_200_characters_is_refused_naming_the_title(
    #[case] title: String,
) {
    validate_agent_notification(&title, A_BODY).assert_refused_naming("title");
}

#[rstest]
#[case::empty(String::new())]
#[case::exactly_2000_characters(chars('b', 2_000))]
#[case::two_thousand_multibyte_characters(chars('🚀', 2_000))]
fn a_body_of_at_most_2000_characters_is_accepted(#[case] body: String) {
    validate_agent_notification(A_TITLE, &body).assert_accepted();
}

#[test]
fn a_body_of_2001_characters_is_refused_naming_the_body() {
    validate_agent_notification(A_TITLE, &chars('b', 2_001)).assert_refused_naming("body");
}
