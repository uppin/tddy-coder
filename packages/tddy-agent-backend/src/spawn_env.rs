//! What the spawn environment said, and what counts as it having said nothing.
//!
//! Every `TDDY_*` variable in the workspace is exported by an outer process — a daemon spawning a
//! jail, a shell wrapper, a systemd unit — and all three of those can export a variable *empty*
//! without meaning to. A blank `TDDY_SOCKET` is not a socket path and a blank join token is not a
//! token, so "set to nothing" and "unset" are the same claim and have to read as the same value.
//!
//! Here rather than in each caller because the workspace had grown three spellings of the rule.

/// An environment variable's value, or `None` when it is unset **or blank**.
///
/// Whitespace counts as blank: a spawn environment that interpolated an empty value into a quoted
/// assignment has still said nothing, and a caller that trusted `" "` would go looking for a socket
/// at a path made of one space.
pub fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// A blank exported variable is the absence of a value, not a value of "". Every caller reads
    /// something an outer process may have exported empty.
    #[test]
    #[serial]
    fn reads_an_empty_variable_as_unset() {
        // Given
        std::env::set_var("TDDY_TEST_BLANK", "");

        // When
        let found = env_non_empty("TDDY_TEST_BLANK");

        // Then
        assert_eq!(found, None);
    }

    /// A variable exported as whitespace is blank too — a spawn environment that interpolated an
    /// empty value into a quoted assignment has still said nothing.
    #[test]
    #[serial]
    fn reads_a_whitespace_only_variable_as_unset() {
        // Given
        std::env::set_var("TDDY_TEST_WHITESPACE", "  \t ");

        // When
        let found = env_non_empty("TDDY_TEST_WHITESPACE");

        // Then
        assert_eq!(found, None);
    }

    #[test]
    #[serial]
    fn reads_a_variable_that_has_a_value() {
        // Given
        std::env::set_var("TDDY_TEST_SET", "/run/tddy.sock");

        // When
        let found = env_non_empty("TDDY_TEST_SET");

        // Then
        assert_eq!(found.as_deref(), Some("/run/tddy.sock"));
    }
}
