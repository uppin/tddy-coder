//! Input handling for the TUI: Select, MultiSelect, and TextInput widgets.
//! All widgets support an "Other (type your own)" escape hatch on option-based questions.

use crossterm::event::KeyEvent;
use tddy_core::ClarificationQuestion;

use crate::tui::state::AppMode;

/// The label shown as the last choice in every Select and MultiSelect question.
pub const OTHER_OPTION_LABEL: &str = "Other (type your own)";

/// Returns all option labels for a clarification question, with `OTHER_OPTION_LABEL` appended.
///
/// The returned vec always has `question.options.len() + 1` elements; the last element is
/// `OTHER_OPTION_LABEL`.
pub fn select_options_with_other(question: &ClarificationQuestion) -> Vec<String> {
    let mut opts: Vec<String> = question.options.iter().map(|o| o.label.clone()).collect();
    opts.push(OTHER_OPTION_LABEL.to_string());
    opts
}

/// Handle a key event while in `AppMode::Select`.
///
/// - Up/Down: move selection cursor.
/// - Enter on a predefined option: returns a mode indicating the answer is ready.
/// - Enter on "Other" (last index) when `typing_other` is false: returns Select with
///   `typing_other: true` so the user can type a custom answer.
/// - Enter when `typing_other` is true: submits the typed text.
/// - Char / Backspace when `typing_other` is true: update `other_text`.
///
/// The caller (`AppState::handle_event`) is responsible for advancing to the next
/// question or returning to Running mode once an answer is confirmed.
pub fn handle_select_key(mode: AppMode, key: KeyEvent) -> AppMode {
    use crossterm::event::KeyCode;
    match mode {
        AppMode::Select {
            question,
            selected,
            other_text,
            typing_other,
        } => {
            let option_count = question.options.len();
            let other_idx = option_count; // "Other" is appended at this index
            match key.code {
                KeyCode::Up => {
                    let new_selected = if selected == 0 {
                        other_idx
                    } else {
                        selected - 1
                    };
                    AppMode::Select {
                        question,
                        selected: new_selected,
                        other_text,
                        typing_other,
                    }
                }
                KeyCode::Down => {
                    let new_selected = if selected >= other_idx {
                        0
                    } else {
                        selected + 1
                    };
                    AppMode::Select {
                        question,
                        selected: new_selected,
                        other_text,
                        typing_other,
                    }
                }
                KeyCode::Enter if !typing_other && selected == other_idx => AppMode::Select {
                    question,
                    selected,
                    other_text,
                    typing_other: true,
                },
                KeyCode::Char(c) if typing_other => {
                    let mut new_text = other_text;
                    new_text.push(c);
                    AppMode::Select {
                        question,
                        selected,
                        other_text: new_text,
                        typing_other,
                    }
                }
                KeyCode::Backspace if typing_other => {
                    let mut new_text = other_text;
                    new_text.pop();
                    AppMode::Select {
                        question,
                        selected,
                        other_text: new_text,
                        typing_other,
                    }
                }
                _ => AppMode::Select {
                    question,
                    selected,
                    other_text,
                    typing_other,
                },
            }
        }
        other => other,
    }
}

/// Handle a key event while in `AppMode::MultiSelect`.
///
/// Returns `(new_mode, Option<answer_string>)`.
/// - Up/Down: move cursor.
/// - Space: toggle checkbox at cursor.
/// - Enter when `typing_other` is false:
///   - If "Other" checkbox is checked and `other_text` is empty: set `typing_other: true`.
///   - Otherwise: collect the answer string (checked labels + other_text if present) and
///     return it as `Some(answer)`.
/// - Char / Backspace when `typing_other` is true: update `other_text`.
/// - Enter when `typing_other` is true: finalize `other_text`, collect answer.
///
/// The answer string format: checked option labels joined by ", ", with `other_text`
/// appended (also ", "-separated) if the "Other" checkbox was checked.
pub fn handle_multiselect_key(mode: AppMode, key: KeyEvent) -> (AppMode, Option<String>) {
    use crossterm::event::KeyCode;
    match mode {
        AppMode::MultiSelect {
            question,
            cursor,
            checked,
            other_text,
            typing_other,
        } => {
            let option_count = question.options.len();
            let other_idx = option_count;
            match key.code {
                KeyCode::Up => {
                    let new_cursor = if cursor == 0 { other_idx } else { cursor - 1 };
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor: new_cursor,
                            checked,
                            other_text,
                            typing_other,
                        },
                        None,
                    )
                }
                KeyCode::Down => {
                    let new_cursor = if cursor >= other_idx { 0 } else { cursor + 1 };
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor: new_cursor,
                            checked,
                            other_text,
                            typing_other,
                        },
                        None,
                    )
                }
                KeyCode::Char(' ') if !typing_other => {
                    let mut new_checked = checked;
                    if cursor <= other_idx && cursor < new_checked.len() {
                        new_checked[cursor] = !new_checked[cursor];
                    }
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor,
                            checked: new_checked,
                            other_text,
                            typing_other,
                        },
                        None,
                    )
                }
                KeyCode::Enter if !typing_other => {
                    let other_checked = checked.get(other_idx).copied().unwrap_or(false);
                    if other_checked && other_text.is_empty() {
                        // Switch to typing custom answer
                        (
                            AppMode::MultiSelect {
                                question,
                                cursor,
                                checked,
                                other_text,
                                typing_other: true,
                            },
                            None,
                        )
                    } else {
                        // Collect answer from checked items (+ other_text if Other checked)
                        let answer = collect_multiselect_answer(&question, &checked, &other_text);
                        (
                            AppMode::MultiSelect {
                                question,
                                cursor,
                                checked,
                                other_text,
                                typing_other,
                            },
                            Some(answer),
                        )
                    }
                }
                KeyCode::Enter if typing_other => {
                    let answer = collect_multiselect_answer(&question, &checked, &other_text);
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor,
                            checked,
                            other_text,
                            typing_other: false,
                        },
                        Some(answer),
                    )
                }
                KeyCode::Char(c) if typing_other => {
                    let mut new_text = other_text;
                    new_text.push(c);
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor,
                            checked,
                            other_text: new_text,
                            typing_other,
                        },
                        None,
                    )
                }
                KeyCode::Backspace if typing_other => {
                    let mut new_text = other_text;
                    new_text.pop();
                    (
                        AppMode::MultiSelect {
                            question,
                            cursor,
                            checked,
                            other_text: new_text,
                            typing_other,
                        },
                        None,
                    )
                }
                _ => (
                    AppMode::MultiSelect {
                        question,
                        cursor,
                        checked,
                        other_text,
                        typing_other,
                    },
                    None,
                ),
            }
        }
        other => (other, None),
    }
}

fn collect_multiselect_answer(
    question: &ClarificationQuestion,
    checked: &[bool],
    other_text: &str,
) -> String {
    let option_count = question.options.len();
    let mut parts: Vec<String> = Vec::new();
    for (i, opt) in question.options.iter().enumerate() {
        if i < checked.len() && checked[i] {
            parts.push(opt.label.clone());
        }
    }
    // Include other_text if the "Other" checkbox is checked
    if checked.get(option_count).copied().unwrap_or(false) && !other_text.is_empty() {
        parts.push(other_text.to_string());
    }
    parts.join(", ")
}

/// Handle a key event while in `AppMode::TextInput`.
///
/// Returns `(new_mode, Option<submitted_text>)`.
/// - Printable chars: inserted at cursor position.
/// - Backspace: delete character before cursor.
/// - Left / Right: move cursor.
/// - Enter: return `Some(input)` to signal submission (non-empty input only).
pub fn handle_text_input_key(
    prompt: String,
    input: String,
    cursor: usize,
    key: KeyEvent,
) -> (AppMode, Option<String>) {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Char(c) => {
            let mut new_input = input;
            new_input.insert(cursor, c);
            let new_cursor = cursor + 1;
            (
                AppMode::TextInput {
                    prompt,
                    input: new_input,
                    cursor: new_cursor,
                },
                None,
            )
        }
        KeyCode::Backspace => {
            if cursor > 0 {
                let mut new_input = input;
                new_input.remove(cursor - 1);
                (
                    AppMode::TextInput {
                        prompt,
                        input: new_input,
                        cursor: cursor - 1,
                    },
                    None,
                )
            } else {
                (
                    AppMode::TextInput {
                        prompt,
                        input,
                        cursor,
                    },
                    None,
                )
            }
        }
        KeyCode::Left => {
            let new_cursor = cursor.saturating_sub(1);
            (
                AppMode::TextInput {
                    prompt,
                    input,
                    cursor: new_cursor,
                },
                None,
            )
        }
        KeyCode::Right => {
            let new_cursor = if cursor < input.len() {
                cursor + 1
            } else {
                input.len()
            };
            (
                AppMode::TextInput {
                    prompt,
                    input,
                    cursor: new_cursor,
                },
                None,
            )
        }
        KeyCode::Enter if !input.is_empty() => {
            let submitted = input.clone();
            (
                AppMode::TextInput {
                    prompt,
                    input,
                    cursor,
                },
                Some(submitted),
            )
        }
        _ => (
            AppMode::TextInput {
                prompt,
                input,
                cursor,
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::AppMode;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tddy_core::QuestionOption;

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    fn backspace_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty())
    }

    fn left_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Left, KeyModifiers::empty())
    }

    fn right_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Right, KeyModifiers::empty())
    }

    fn make_question(text: &str, options: &[&str], multi: bool) -> ClarificationQuestion {
        ClarificationQuestion {
            header: "Header".to_string(),
            question: text.to_string(),
            options: options
                .iter()
                .map(|o| QuestionOption {
                    label: o.to_string(),
                    description: String::new(),
                })
                .collect(),
            multi_select: multi,
        }
    }

    /// AC5: Select mode's option list is original options + 1, with OTHER_OPTION_LABEL last.
    #[test]
    fn test_select_mode_appends_other_option() {
        let q = make_question("Choose one?", &["Option A", "Option B"], false);
        let original_count = q.options.len();

        let options = select_options_with_other(&q);

        assert_eq!(
            options.len(),
            original_count + 1,
            "options list must be exactly original count + 1 for 'Other'"
        );
        assert_eq!(
            options.last().unwrap(),
            OTHER_OPTION_LABEL,
            "last option must be '{}'",
            OTHER_OPTION_LABEL
        );
        // Original options preserved in order
        assert_eq!(options[0], "Option A");
        assert_eq!(options[1], "Option B");
    }

    /// AC6: Pressing Enter when the cursor is on "Other" switches to typing_other=true.
    #[test]
    fn test_select_other_switches_to_text() {
        let q = make_question("Choose one?", &["Option A", "Option B"], false);
        // "Other" is appended at index == original options count
        let other_index = q.options.len(); // index 2

        let mode = AppMode::Select {
            question: q,
            selected: other_index,
            other_text: String::new(),
            typing_other: false,
        };

        let next = handle_select_key(mode, enter_key());

        match next {
            AppMode::Select {
                typing_other: true, ..
            } => {} // correct: cursor on "Other" + Enter → enter typing sub-mode
            other => panic!(
                "expected Select {{ typing_other: true, .. }}, got {:?}",
                other
            ),
        }
    }

    /// AC7: MultiSelect with "Other" checked and other_text pre-filled; Enter collects
    /// the answer with both the checked predefined label and the custom text, excluding
    /// unchecked labels.
    #[test]
    fn test_multiselect_other_with_checked() {
        let q = make_question("Choose all?", &["Alpha", "Beta"], true);
        // checked indices: Alpha(0)=true, Beta(1)=false, Other(2)=true
        let checked = vec![true, false, true];

        let mode = AppMode::MultiSelect {
            question: q,
            cursor: 0,
            checked,
            other_text: "custom answer".to_string(),
            typing_other: false,
        };

        let (_next_mode, answer_opt) = handle_multiselect_key(mode, enter_key());
        let answer = answer_opt.expect("expected a collected answer when Enter is pressed");

        assert!(
            answer.contains("Alpha"),
            "answer must include checked option 'Alpha': {}",
            answer
        );
        assert!(
            answer.contains("custom answer"),
            "answer must include Other text 'custom answer': {}",
            answer
        );
        assert!(
            !answer.contains("Beta"),
            "answer must NOT include unchecked option 'Beta': {}",
            answer
        );
    }

    /// AC6 + TextInput: character insertion, backspace, left/right cursor movement,
    /// and Enter-to-submit all work correctly.
    #[test]
    fn test_text_input_handles_backspace_and_cursor() {
        let prompt = "Enter text:".to_string();
        let mut input = String::new();
        let mut cursor = 0usize;

        // Type "hello"
        for c in "hello".chars() {
            let (mode, submitted) =
                handle_text_input_key(prompt.clone(), input.clone(), cursor, char_key(c));
            assert!(submitted.is_none(), "typing should not submit");
            match mode {
                AppMode::TextInput {
                    input: new_input,
                    cursor: new_cursor,
                    ..
                } => {
                    input = new_input;
                    cursor = new_cursor;
                }
                other => panic!("expected TextInput after typing, got {:?}", other),
            }
        }
        assert_eq!(input, "hello");
        assert_eq!(cursor, 5);

        // Backspace removes last char: "hello" → "hell", cursor 5 → 4
        let (mode, submitted) =
            handle_text_input_key(prompt.clone(), input.clone(), cursor, backspace_key());
        assert!(submitted.is_none(), "backspace must not submit");
        match mode {
            AppMode::TextInput {
                input: new_input,
                cursor: new_cursor,
                ..
            } => {
                assert_eq!(new_input, "hell", "backspace must remove last char");
                assert_eq!(new_cursor, 4, "cursor must move back after backspace");
            }
            other => panic!("expected TextInput after backspace, got {:?}", other),
        }

        // Left key moves cursor without changing input
        let (mode, _) = handle_text_input_key(prompt.clone(), "hell".to_string(), 4, left_key());
        match mode {
            AppMode::TextInput {
                input: new_input,
                cursor: new_cursor,
                ..
            } => {
                assert_eq!(new_cursor, 3, "left key must decrement cursor");
                assert_eq!(new_input, "hell", "left key must not modify input");
            }
            other => panic!("expected TextInput after left key, got {:?}", other),
        }

        // Right key moves cursor right
        let (mode, _) = handle_text_input_key(prompt.clone(), "hell".to_string(), 3, right_key());
        match mode {
            AppMode::TextInput {
                cursor: new_cursor, ..
            } => {
                assert_eq!(new_cursor, 4, "right key must increment cursor");
            }
            other => panic!("expected TextInput after right key, got {:?}", other),
        }

        // Enter on non-empty input submits the text
        let (_, submitted) =
            handle_text_input_key(prompt.clone(), "hello".to_string(), 5, enter_key());
        assert_eq!(
            submitted,
            Some("hello".to_string()),
            "Enter must submit the current input text"
        );
    }
}
