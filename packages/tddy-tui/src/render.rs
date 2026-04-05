//! Frame rendering: draw activity log, status bar, prompt bar.

use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use tddy_core::{ActivityKind, AppMode, PresenterState};

use crate::feature_input_buffer::FeaturePromptSegment;
use crate::layout::{
    clarification_questions_top, debug_log_height, inbox_height,
    layout_chunks_with_inbox_maybe_top, prompt_chunk_height_including_rule, question_height,
};
use crate::mouse_map::{
    enter_button_rect, stop_button_rect, LayoutAreas, ENTER_BUTTON_COLS, STOP_BUTTON_COLS,
};
use crate::status_bar_activity::{
    activity_prefix_char_for_draw, display_elapsed_for_goal_row, status_activity_is_agent_active,
};
use crate::ui::{
    first_hyphen_segment_of_workflow_session_id, format_status_bar_idle,
    format_status_bar_with_activity_prefix, prepend_activity_to_status_line,
    status_bar_style_for_goal,
};
use crate::view_state::{InboxFocus, ViewState};

/// Line count for a [`Paragraph`] with the same wrap settings as the Markdown viewer (must match draw).
fn markdown_paragraph_wrapped_line_count(text: &Text<'_>, width: u16) -> usize {
    if width < 1 {
        return 1;
    }
    let n = Paragraph::new(text.clone())
        .wrap(Wrap { trim: true })
        .line_count(width);
    n.max(1)
}

/// Trailing Approve/Reject appended to plan markdown once the user reaches the document end.
///
/// Uses a single wrapped row (plus spacer line) so both labels stay in the last viewport together on
/// typical widths; this matches the prior one-line footer layout while living inside scroll content.
fn markdown_plan_action_tail_lines(markdown_end_button_selected: usize) -> Vec<Line<'static>> {
    let approve_prefix = if markdown_end_button_selected == 0 {
        "> "
    } else {
        "  "
    };
    let reject_prefix = if markdown_end_button_selected == 1 {
        "> "
    } else {
        "  "
    };
    let approve_style = if markdown_end_button_selected == 0 {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    let reject_style = if markdown_end_button_selected == 1 {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{}Approve", approve_prefix), approve_style),
            Span::raw("     "),
            Span::styled(format!("{}Reject", reject_prefix), reject_style),
        ]),
    ]
}

fn markdown_viewer_prompt_for_plan_approval(
    state: &PresenterState,
    view_state: &ViewState,
) -> String {
    log::debug!(
        "markdown_viewer_prompt: pending={} markdown_end_button_selected={}",
        state.plan_refinement_pending,
        view_state.markdown_end_button_selected
    );
    if state.plan_refinement_pending {
        if view_state.plan_refinement_input.is_empty() {
            "> Type refinement feedback and press Enter...".to_string()
        } else {
            format!("> {}", view_state.plan_refinement_input)
        }
    } else if !view_state.plan_refinement_input.is_empty() {
        format!("> {}", view_state.plan_refinement_input)
    } else if view_state.markdown_at_end && view_state.markdown_end_button_selected == 1 {
        "Type refinement feedback in the prompt below, then press Enter  |  Q/Esc to close"
            .to_string()
    } else if view_state.markdown_at_end {
        "Q or Esc to close  |  Alt+A=Approve  Alt+R=Reject/refine  |  PgUp/PgDn scroll".to_string()
    } else {
        "Q or Esc to close  |  PgUp/PgDn scroll (read to the end for Approve / Reject)".to_string()
    }
}

/// Return the prompt bar text for the current mode and view state.
fn prompt_text(state: &PresenterState, view_state: &ViewState) -> String {
    match &state.mode {
        AppMode::FeatureInput => {
            let prefix = "> ";
            if view_state.feature_slash_open {
                format!(
                    "{}Slash menu  Up/Down  Enter apply  Esc cancel — {}",
                    prefix,
                    view_state.feature_edit.display()
                )
            } else if view_state.feature_edit.display().is_empty() {
                format!("{}Type your feature description and press Enter...", prefix)
            } else {
                format!("{}{}", prefix, view_state.feature_edit.display())
            }
        }
        AppMode::Running => {
            if view_state.running_input.is_empty() {
                "> Queue a follow-up prompt...".to_string()
            } else {
                format!("> {}", view_state.running_input)
            }
        }
        AppMode::Done => "> Workflow complete. Press Enter to exit.".to_string(),
        AppMode::Select { .. } => {
            if view_state.select_typing_other {
                if view_state.select_other_text.is_empty() {
                    "> Type your answer and press Enter...".to_string()
                } else {
                    format!("> {}", view_state.select_other_text)
                }
            } else {
                "Up/Down navigate  Enter select".to_string()
            }
        }
        AppMode::MultiSelect { .. } => {
            if view_state.multiselect_typing_other {
                if view_state.multiselect_other_text.is_empty() {
                    "> Type your answer and press Enter...".to_string()
                } else {
                    format!("> {}", view_state.multiselect_other_text)
                }
            } else {
                "Up/Down navigate  Space toggle  Enter submit".to_string()
            }
        }
        AppMode::TextInput { .. } => {
            if view_state.text_input.is_empty() {
                "> Type your answer and press Enter...".to_string()
            } else {
                format!("> {}", view_state.text_input)
            }
        }
        AppMode::DocumentReview { .. } => "Up/Down navigate  Enter select".to_string(),
        AppMode::MarkdownViewer { .. } => {
            markdown_viewer_prompt_for_plan_approval(state, view_state)
        }
        AppMode::ErrorRecovery { .. } => "Up/Down navigate  Enter select".to_string(),
    }
}

/// Dark navy fill behind `/skill-name` tokens in the feature prompt; label text is white.
fn feature_skill_token_style() -> Style {
    Style::default().bg(Color::Rgb(18, 36, 68)).fg(Color::White)
}

/// Activity pane style for user-submitted / queued prompt lines ([`ActivityKind::UserPrompt`]).
fn activity_log_user_prompt_line_style() -> Style {
    Style::default()
        .bg(Color::Rgb(85, 85, 85))
        .fg(Color::Rgb(255, 255, 255))
        .add_modifier(Modifier::BOLD)
}

/// Fixed height (terminal rows) for each [`ActivityKind::UserPrompt`] text block in the activity log.
const USER_PROMPT_ACTIVITY_ROWS: usize = 3;
/// Horizontal margin (columns) on **each** side of the styled block (left and right).
const USER_PROMPT_MARGIN_H: u16 = 1;
/// Blank lines above and below the styled block (top and bottom margin).
const USER_PROMPT_MARGIN_V: usize = 1;

/// Hard-wrap `text` to at most `line_width` characters per line; `\n` starts a new line.
fn hard_wrap_activity_lines(text: &str, line_width: usize) -> Vec<String> {
    if line_width == 0 {
        return vec![];
    }
    let mut lines = Vec::new();
    for part in text.split('\n') {
        if part.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut remaining = part;
        while !remaining.is_empty() {
            let mut end_byte = 0;
            for (n, (i, ch)) in remaining.char_indices().enumerate() {
                if n >= line_width {
                    break;
                }
                end_byte = i + ch.len_utf8();
            }
            lines.push(remaining[..end_byte].to_string());
            remaining = &remaining[end_byte..];
        }
    }
    if lines.is_empty() && text.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn ellipsis_tail_to_width(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    let take = max_chars.saturating_sub(1);
    format!("{}…", s.chars().take(take).collect::<String>())
}

fn pad_row_to_char_width(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        return s.to_string();
    }
    let mut out = s.to_string();
    out.extend(std::iter::repeat_n(' ', width - n));
    out
}

/// Three rows: **row 1 empty**, row 2–3 hold wrapped text (second line uses ellipsis if more remains).
fn user_prompt_activity_row_strings(
    text: &str,
    content_width: usize,
) -> [String; USER_PROMPT_ACTIVITY_ROWS] {
    let wrapped = hard_wrap_activity_lines(text, content_width);
    let row_top = String::new();

    match wrapped.len() {
        0 => [row_top, String::new(), String::new()],
        1 => [row_top, wrapped[0].clone(), String::new()],
        2 => [row_top, wrapped[0].clone(), wrapped[1].clone()],
        _ => {
            let tail = wrapped[1..].concat();
            [
                row_top,
                wrapped[0].clone(),
                ellipsis_tail_to_width(&tail, content_width),
            ]
        }
    }
}

/// Character-wrap prompt spans while preserving per-character styles (matches `prompt_height`).
fn wrap_spans_to_prompt_lines(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(spans)];
    }
    let mut flat: Vec<(Style, char)> = Vec::new();
    for sp in spans {
        let st = sp.style;
        for ch in sp.content.chars() {
            flat.push((st, ch));
        }
    }
    if flat.is_empty() {
        return vec![Line::default()];
    }
    let mut lines = Vec::new();
    for row in flat.chunks(width) {
        let mut line_spans: Vec<Span> = Vec::new();
        let mut cur_st = row[0].0;
        let mut buf = String::new();
        for (st, ch) in row {
            if *st != cur_st {
                if !buf.is_empty() {
                    line_spans.push(Span::styled(std::mem::take(&mut buf), cur_st));
                }
                cur_st = *st;
            }
            buf.push(*ch);
        }
        if !buf.is_empty() {
            line_spans.push(Span::styled(buf, cur_st));
        }
        lines.push(Line::from(line_spans));
    }
    lines
}

/// Absolute terminal cell for the text insert cursor after [`draw`]'s char-chunked prompt `Paragraph`.
///
/// `byte_cursor` is a UTF-8 index into the same string passed to the prompt widget; must fall on a
/// character boundary.
pub(crate) fn terminal_position_for_byte_cursor_in_char_wrapped_prompt(
    prompt_text: &str,
    byte_cursor: usize,
    area: Rect,
) -> Option<ratatui::layout::Position> {
    if byte_cursor > prompt_text.len() || !prompt_text.is_char_boundary(byte_cursor) {
        return None;
    }
    let w = area.width as usize;
    if w == 0 {
        return None;
    }
    let n = prompt_text[..byte_cursor].chars().count();
    let row = n / w;
    let col = n % w;
    let y = area.y.checked_add(row as u16)?;
    let x = area.x.checked_add(col as u16)?;
    Some(ratatui::layout::Position::new(x, y))
}

/// Where Approve/Reject live relative to the markdown scroll region (PRD plan tail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkdownPlanActionLayout {
    /// Pre-PRD: always reserve footer lines under the body (kept for layout tests; not used in draw).
    #[allow(dead_code)]
    FixedFooterAlways,
    /// PRD: embed actions as trailing scroll content once `markdown_at_end`.
    TailWithDocumentWhenAtEnd,
}

/// Plan actions follow the markdown tail once the user has scrolled to the document end.
pub(crate) fn markdown_plan_action_layout_for_view(
    markdown_at_end: bool,
) -> MarkdownPlanActionLayout {
    let layout = MarkdownPlanActionLayout::TailWithDocumentWhenAtEnd;
    log::trace!(
        "markdown_plan_action_layout_for_view: markdown_at_end={} -> {:?} (fixed footer retired)",
        markdown_at_end,
        layout
    );
    layout
}

/// Injects [`PresenterState::active_worktree_display`] into the built status line (PRD).
///
/// Inserts the segment before the `Goal:` token when present so spinner and session ordering stay intact.
pub(crate) fn inject_worktree_into_status_line(line: String, worktree: Option<&str>) -> String {
    let Some(w) = worktree.map(str::trim).filter(|s| !s.is_empty()) else {
        log::trace!("inject_worktree_into_status_line: no worktree (unchanged line)");
        return line;
    };
    log::debug!(
        "inject_worktree_into_status_line: weaving display {:?} into status row",
        w
    );
    if let Some(idx) = line.find("Goal:") {
        let head = line[..idx].trim_end();
        format!("{head} │ {w} │ {}", &line[idx..])
    } else {
        format!("{line} │ {w}")
    }
}

fn snap_utf8_byte_index(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Hardware cursor position after drawing the char-wrapped prompt, when the user is editing text.
pub(crate) fn editing_prompt_cursor_position(
    state: &PresenterState,
    view_state: &ViewState,
    prompt_bar: Rect,
) -> Option<ratatui::layout::Position> {
    const PFX: &str = "> ";
    let (prompt_text, byte_cursor) = match &state.mode {
        AppMode::FeatureInput => {
            if view_state.feature_slash_open {
                log::trace!("editing_prompt_cursor_position: slash menu open — no caret");
                return None;
            }
            let d = view_state.feature_edit.display();
            if d.is_empty() {
                let t = format!("{PFX}Type your feature description and press Enter...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{d}");
                let inner = snap_utf8_byte_index(&d, view_state.feature_edit.cursor);
                (t, PFX.len() + inner)
            }
        }
        AppMode::Running => {
            if view_state.running_input.is_empty() {
                let t = format!("{PFX}Queue a follow-up prompt...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{}", view_state.running_input);
                let inner =
                    snap_utf8_byte_index(&view_state.running_input, view_state.running_cursor);
                (t, PFX.len() + inner)
            }
        }
        AppMode::Select { .. } if view_state.select_typing_other => {
            if view_state.select_other_text.is_empty() {
                let t = format!("{PFX}Type your answer and press Enter...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{}", view_state.select_other_text);
                (t, PFX.len() + view_state.select_other_text.len())
            }
        }
        AppMode::MultiSelect { .. } if view_state.multiselect_typing_other => {
            if view_state.multiselect_other_text.is_empty() {
                let t = format!("{PFX}Type your answer and press Enter...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{}", view_state.multiselect_other_text);
                (t, PFX.len() + view_state.multiselect_other_text.len())
            }
        }
        AppMode::TextInput { .. } => {
            if view_state.text_input.is_empty() {
                let t = format!("{PFX}Type your answer and press Enter...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{}", view_state.text_input);
                let inner =
                    snap_utf8_byte_index(&view_state.text_input, view_state.text_input_cursor);
                (t, PFX.len() + inner)
            }
        }
        AppMode::MarkdownViewer { .. } if state.plan_refinement_pending => {
            let inp = &view_state.plan_refinement_input;
            if inp.is_empty() {
                let t = format!("{PFX}Type refinement feedback and press Enter...");
                (t, PFX.len())
            } else {
                let t = format!("{PFX}{inp}");
                let inner = snap_utf8_byte_index(inp, view_state.plan_refinement_cursor);
                (t, PFX.len() + inner)
            }
        }
        _ => {
            log::trace!(
                "editing_prompt_cursor_position: mode {:?} — no hardware caret",
                state.mode
            );
            return None;
        }
    };
    log::debug!(
        "editing_prompt_cursor_position: prompt_h={} byte_cursor={} (chars={})",
        prompt_bar.height,
        byte_cursor,
        prompt_text.chars().count()
    );
    terminal_position_for_byte_cursor_in_char_wrapped_prompt(&prompt_text, byte_cursor, prompt_bar)
}

/// Draw the TUI layout: activity log, status bar, prompt bar.
/// When `debug` is true, the debug log area is shown; otherwise it is hidden.
const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];

/// Builds the single-line status bar text for the primary and Virtual TUI [`draw`] path.
///
/// Leading content is the cycling spinner (agent-active) or 1 Hz idle dot (clarification wait), then
/// the workflow session segment and `Goal:` … tail.
fn status_bar_text_for_draw(state: &PresenterState, view_state: &mut ViewState) -> String {
    view_state.sync_status_bar_with_presenter(state);
    let agent_active = status_activity_is_agent_active(&state.mode);
    let spinner_char = activity_prefix_char_for_draw(&state.mode, view_state, SPINNER_FRAMES);
    log::trace!(
        "status_bar_text_for_draw: agent_active={} tick={} prefix={:?} workflow_session_id={:?}",
        agent_active,
        view_state.spinner_tick,
        spinner_char,
        state
            .workflow_session_id
            .as_deref()
            .map(truncate_session_id_for_log)
    );
    let segment = first_hyphen_segment_of_workflow_session_id(state.workflow_session_id.as_deref());
    let segment_str = segment.as_ref();
    let line = match (&state.current_goal, &state.current_state) {
        (Some(goal), Some(s)) => {
            let elapsed = display_elapsed_for_goal_row(state, view_state);
            format_status_bar_with_activity_prefix(
                spinner_char,
                segment_str,
                goal,
                s,
                elapsed,
                &state.agent,
                &state.model,
            )
        }
        _ => {
            let idle_tail = format_status_bar_idle(&state.agent, &state.model);
            prepend_activity_to_status_line(spinner_char, segment_str, &idle_tail)
        }
    };
    inject_worktree_into_status_line(line, state.active_worktree_display.as_deref())
}

/// Truncate long session ids in trace logs (correlation id, not a secret, but avoid full dumps).
/// PRD FR1: last line of the activity pane shows the user-submitted prompt (white on dark grey).
/// Bottom border of the prompt pane: full-width `U+2500` light horizontal rule.
fn paint_prompt_bottom_horizontal_rule(frame: &mut Frame, prompt_bar: Rect) {
    if prompt_bar.height < 2 {
        return;
    }
    let y = prompt_bar.y + prompt_bar.height - 1;
    let buf = frame.buffer_mut();
    const H: &str = "\u{2500}";
    for dx in 0..prompt_bar.width {
        let x = prompt_bar.x.saturating_add(dx);
        if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
            cell.set_symbol(H);
        }
    }
}

fn paint_user_prompt_activity_strip(frame: &mut Frame, activity_log: Rect, running_input: &str) {
    crate::red_phase::tddy_marker("M004", "render::paint_user_prompt_activity_strip");
    log::debug!(
        "paint_user_prompt_activity_strip: activity_log={activity_log:?} len={}",
        running_input.chars().count()
    );
    if activity_log.height == 0 || activity_log.width == 0 {
        log::trace!("paint_user_prompt_activity_strip: skip (zero activity area)");
        return;
    }
    let strip_y = activity_log.y + activity_log.height - 1;
    let label = format!("> {}", running_input);
    let style = Style::default().fg(Color::White).bg(Color::DarkGray);
    let buf = frame.buffer_mut();
    for (i, ch) in label.chars().enumerate() {
        let x = activity_log.x.saturating_add(i as u16);
        if x >= activity_log.x + activity_log.width {
            break;
        }
        if let Some(cell) = buf.cell_mut(Position::new(x, strip_y)) {
            cell.set_symbol(ch.to_string().as_str());
            cell.set_style(style);
        }
    }
    log::trace!(
        "paint_user_prompt_activity_strip: painted strip_y={strip_y} prefix_len={}",
        label.chars().count().min(activity_log.width as usize)
    );
}

fn truncate_session_id_for_log(id: &str) -> String {
    const MAX: usize = 12;
    if id.len() <= MAX {
        id.to_string()
    } else {
        format!("{}…", &id[..MAX])
    }
}

/// Paints the Enter affordance on the trailing 3 columns from the first row **below the status bar**
/// through prompt text and footer (see [`enter_button_rect`]). Top row draws `┌─┐` on the separator
/// band; U+23CE sits on the first prompt text row, not on that top border. The bottom prompt row is
/// the horizontal rule and is excluded from height.
fn paint_enter_affordance(frame: &mut Frame, areas: &LayoutAreas) {
    crate::red_phase::tddy_marker("M003", "render::paint_enter_affordance");
    // Long echo / vt100 substring tests (`tddy-e2e` grpc_terminal_rpc) need a stable flattened
    // screen; the overlay is opt-out via env for those tests only (any defined value disables paint).
    if std::env::var_os("TDDY_E2E_NO_ENTER_AFFORDANCE").is_some() {
        log::trace!("paint_enter_affordance: skipped (TDDY_E2E_NO_ENTER_AFFORDANCE)");
        return;
    }
    let r = enter_button_rect(areas);
    log::debug!(
        "paint_enter_affordance: rect={r:?} footer_bar={:?}",
        areas.footer_bar
    );
    if r.width == 0 || r.height == 0 {
        return;
    }
    let frame_area = frame.area();
    if r.x.saturating_add(r.width) > frame_area.width
        || r.y.saturating_add(r.height) > frame_area.height
    {
        log::debug!("paint_enter_affordance: rect outside frame, skip");
        return;
    }
    let buf = frame.buffer_mut();
    const RETURN_SYMBOL: &str = "\u{23CE}";
    // Unicode box drawing (light); 3 columns wide: corners + horizontal + verticals.
    const BOX_H: &str = "\u{2500}"; // ─
    const BOX_V: &str = "\u{2502}"; // │
    const BOX_TL: &str = "\u{250C}"; // ┌
    const BOX_TR: &str = "\u{2510}"; // ┐
    const BOX_BL: &str = "\u{2514}"; // └
    const BOX_BR: &str = "\u{2518}"; // ┘
    let sb = areas.status_bar;
    let pb = areas.prompt_bar;
    let above_prompt = if sb.height > 0 {
        pb.y.saturating_sub(sb.y.saturating_add(sb.height))
    } else {
        0
    };
    let use_separator_top = above_prompt >= 1;
    let h = r.height;
    for dy in 0..h {
        for dx in 0..ENTER_BUTTON_COLS {
            let sym: &str = if h == 1 {
                match dx {
                    0 => BOX_TL,
                    1 => RETURN_SYMBOL,
                    2 => BOX_TR,
                    _ => " ",
                }
            } else if use_separator_top {
                if dy == 0 {
                    match dx {
                        0 => BOX_TL,
                        1 => BOX_H,
                        2 => BOX_TR,
                        _ => " ",
                    }
                } else if dy == h - 1 {
                    match dx {
                        0 => BOX_BL,
                        1 => {
                            if h == 2 {
                                RETURN_SYMBOL
                            } else {
                                BOX_H
                            }
                        }
                        2 => BOX_BR,
                        _ => " ",
                    }
                } else if dx == 0 || dx == 2 {
                    BOX_V
                } else if dy == above_prompt {
                    RETURN_SYMBOL
                } else {
                    " "
                }
            } else if dy == 0 {
                match dx {
                    0 => BOX_TL,
                    1 => {
                        if h == 2 {
                            RETURN_SYMBOL
                        } else {
                            BOX_H
                        }
                    }
                    2 => BOX_TR,
                    _ => " ",
                }
            } else if dy == h - 1 {
                match dx {
                    0 => BOX_BL,
                    1 => BOX_H,
                    2 => BOX_BR,
                    _ => " ",
                }
            } else if dx == 0 || dx == 2 {
                BOX_V
            } else if dy == 1 && h >= 3 {
                RETURN_SYMBOL
            } else {
                " "
            };
            let x = r.x + dx;
            let y = r.y + dy;
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_symbol(sym);
            }
        }
    }
    log::trace!(
        "paint_enter_affordance: painted {}×{} overlay at y={}",
        r.width,
        r.height,
        r.y
    );
}

/// Paints the Stop affordance (red U+25A0) to the right of Enter (see [`stop_button_rect`]).
/// Same geometry rules as [`paint_enter_affordance`]; skipped under `TDDY_E2E_NO_ENTER_AFFORDANCE`.
fn paint_stop_affordance(frame: &mut Frame, areas: &LayoutAreas) {
    if std::env::var_os("TDDY_E2E_NO_ENTER_AFFORDANCE").is_some() {
        log::trace!("paint_stop_affordance: skipped (TDDY_E2E_NO_ENTER_AFFORDANCE)");
        return;
    }
    let r = stop_button_rect(areas);
    if r.width == 0 || r.height == 0 {
        return;
    }
    let frame_area = frame.area();
    if r.x.saturating_add(r.width) > frame_area.width
        || r.y.saturating_add(r.height) > frame_area.height
    {
        log::debug!("paint_stop_affordance: rect outside frame, skip");
        return;
    }
    let buf = frame.buffer_mut();
    const STOP_SYMBOL: &str = "■";
    const BOX_H: &str = "─";
    const BOX_V: &str = "│";
    const BOX_TL: &str = "┌";
    const BOX_TR: &str = "┐";
    const BOX_BL: &str = "└";
    const BOX_BR: &str = "┘";
    let stop_style = Style::default().fg(Color::Red);
    let sb = areas.status_bar;
    let pb = areas.prompt_bar;
    let above_prompt = if sb.height > 0 {
        pb.y.saturating_sub(sb.y.saturating_add(sb.height))
    } else {
        0
    };
    let use_separator_top = above_prompt >= 1;
    let h = r.height;
    for dy in 0..h {
        for dx in 0..STOP_BUTTON_COLS {
            let sym: &str = if h == 1 {
                match dx {
                    0 => BOX_TL,
                    1 => STOP_SYMBOL,
                    2 => BOX_TR,
                    _ => " ",
                }
            } else if use_separator_top {
                if dy == 0 {
                    match dx {
                        0 => BOX_TL,
                        1 => BOX_H,
                        2 => BOX_TR,
                        _ => " ",
                    }
                } else if dy == h - 1 {
                    match dx {
                        0 => BOX_BL,
                        1 => {
                            if h == 2 {
                                STOP_SYMBOL
                            } else {
                                BOX_H
                            }
                        }
                        2 => BOX_BR,
                        _ => " ",
                    }
                } else if dx == 0 || dx == 2 {
                    BOX_V
                } else if dy == above_prompt {
                    STOP_SYMBOL
                } else {
                    " "
                }
            } else if dy == 0 {
                match dx {
                    0 => BOX_TL,
                    1 => {
                        if h == 2 {
                            STOP_SYMBOL
                        } else {
                            BOX_H
                        }
                    }
                    2 => BOX_TR,
                    _ => " ",
                }
            } else if dy == h - 1 {
                match dx {
                    0 => BOX_BL,
                    1 => BOX_H,
                    2 => BOX_BR,
                    _ => " ",
                }
            } else if dx == 0 || dx == 2 {
                BOX_V
            } else if dy == 1 && h >= 3 {
                STOP_SYMBOL
            } else {
                " "
            };
            let x = r.x + dx;
            let y = r.y + dy;
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_symbol(sym);
                cell.set_style(stop_style);
            }
        }
    }
    log::trace!(
        "paint_stop_affordance: painted {}×{} overlay at y={}",
        r.width,
        r.height,
        r.y
    );
}

/// Draw the TUI. When `layout_areas` is Some, stores the layout rects for mouse hit-testing.
pub fn draw(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &mut ViewState,
    debug: bool,
    layout_areas: Option<&mut LayoutAreas>,
) {
    let is_running = matches!(state.mode, AppMode::Running);
    let inbox_h = inbox_height(state.inbox.len(), is_running);
    let question_h = question_height(&state.mode);
    let slash_h = if matches!(state.mode, AppMode::FeatureInput) {
        view_state.feature_slash_dynamic_height()
    } else {
        0
    };
    let dynamic_h = question_h.max(inbox_h).max(slash_h);
    let debug_logs = if debug {
        tddy_core::get_buffered_logs()
    } else {
        vec![]
    };
    let debug_h = debug_log_height(debug_logs.len());

    let area = frame.area();
    let prompt_text_str = prompt_text(state, view_state);
    let text_len = prompt_text_str.chars().count().min(u16::MAX as usize) as u16;
    let prompt_h = prompt_chunk_height_including_rule(text_len, area.width, area.height);

    let questions_top = clarification_questions_top(&state.mode);
    let (
        activity_log,
        _status_spacer,
        dynamic_area,
        status_bar,
        _status_prompt_gap,
        debug_log,
        prompt_bar,
        footer_bar,
    ) = layout_chunks_with_inbox_maybe_top(area, dynamic_h, debug_h, prompt_h, questions_top);

    log::debug!("draw: layout status={status_bar:?} prompt={prompt_bar:?} footer={footer_bar:?}");

    let base_for_chrome = LayoutAreas {
        activity_log,
        dynamic_area,
        status_bar,
        prompt_bar,
        footer_bar,
        enter_pane: ratatui::layout::Rect::default(),
        stop_pane: ratatui::layout::Rect::default(),
    };
    let enter_hit = enter_button_rect(&base_for_chrome);
    let stop_hit = stop_button_rect(&base_for_chrome);

    if let Some(areas) = layout_areas {
        *areas = LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: enter_hit,
            stop_pane: stop_hit,
        };
    }

    if activity_log.height > 0 {
        match &state.mode {
            AppMode::DocumentReview { content } => {
                const MENU_LINES: u16 = 4;
                let menu_h = MENU_LINES.min(activity_log.height);
                let body_h = activity_log.height.saturating_sub(menu_h);
                if body_h > 0 {
                    let body_area =
                        Rect::new(activity_log.x, activity_log.y, activity_log.width, body_h);
                    let text = tui_markdown::from_str(content);
                    let widget = Paragraph::new(text).scroll((view_state.scroll_offset as u16, 0));
                    frame.render_widget(widget, body_area);
                }
                if menu_h > 0 {
                    let menu_y = activity_log.y.saturating_add(body_h);
                    let menu_area = Rect::new(activity_log.x, menu_y, activity_log.width, menu_h);
                    render_document_review(frame, state, view_state, menu_area);
                }
            }
            AppMode::MarkdownViewer { content } => {
                let body_area = activity_log;
                let body_h = body_area.height;
                let w = body_area.width;
                let text_md = tui_markdown::from_str(content);
                let lines_md = markdown_paragraph_wrapped_line_count(&text_md, w);
                let visible = body_h as usize;
                let max_scroll_md = if visible == 0 {
                    0
                } else {
                    lines_md.saturating_sub(visible)
                };
                let scroll_raw = view_state.markdown_scroll_offset;
                let scrolled_to_md_bottom = visible == 0
                    || lines_md <= visible
                    || (max_scroll_md > 0 && scroll_raw >= max_scroll_md);
                view_state.markdown_at_end = scrolled_to_md_bottom;
                let _plan_layout = markdown_plan_action_layout_for_view(view_state.markdown_at_end);
                log::debug!(
                    "render: MarkdownViewer lines_md={} visible={} max_scroll_md={} scroll_raw={} at_end={}",
                    lines_md,
                    visible,
                    max_scroll_md,
                    scroll_raw,
                    view_state.markdown_at_end
                );

                let display_text = if scrolled_to_md_bottom && body_h > 0 {
                    let mut lines = text_md.lines.clone();
                    lines.extend(markdown_plan_action_tail_lines(
                        view_state.markdown_end_button_selected,
                    ));
                    Text::from(lines)
                } else {
                    text_md.clone()
                };

                let total_lines = markdown_paragraph_wrapped_line_count(&display_text, w);
                let max_scroll = if visible == 0 {
                    0
                } else {
                    total_lines.saturating_sub(visible)
                };

                let mut scroll = scroll_raw;
                if !scrolled_to_md_bottom {
                    scroll = scroll.min(max_scroll_md);
                } else {
                    scroll = scroll.min(max_scroll);
                    if max_scroll > max_scroll_md && scroll_raw >= max_scroll_md {
                        scroll = max_scroll;
                    }
                    if max_scroll_md == 0 && max_scroll > 0 {
                        scroll = max_scroll;
                    }
                }
                view_state.markdown_scroll_offset = scroll;

                if body_h > 0 {
                    let widget = Paragraph::new(display_text)
                        .wrap(Wrap { trim: true })
                        .scroll((view_state.markdown_scroll_offset as u16, 0));
                    frame.render_widget(widget, body_area);
                }
            }
            _ => {
                let mut display_lines: Vec<Line> = Vec::new();
                let pane_w = activity_log.width;
                for e in &state.activity_log {
                    if e.kind == ActivityKind::UserPrompt {
                        if pane_w == 0 {
                            continue;
                        }
                        let style = activity_log_user_prompt_line_style();
                        let margin_h = USER_PROMPT_MARGIN_H.min(pane_w / 2);
                        let content_w = pane_w.saturating_sub(margin_h.saturating_mul(2)) as usize;

                        for _ in 0..USER_PROMPT_MARGIN_V {
                            display_lines.push(Line::default());
                        }

                        if content_w == 0 {
                            for _ in 0..USER_PROMPT_ACTIVITY_ROWS {
                                display_lines.push(Line::default());
                            }
                        } else {
                            let rows = user_prompt_activity_row_strings(&e.text, content_w);
                            let pad_side = " ".repeat(margin_h as usize);
                            for row in rows {
                                let padded = pad_row_to_char_width(&row, content_w);
                                display_lines.push(Line::from(vec![
                                    Span::raw(pad_side.clone()),
                                    Span::styled(padded, style),
                                    Span::raw(pad_side.clone()),
                                ]));
                            }
                        }

                        for _ in 0..USER_PROMPT_MARGIN_V {
                            display_lines.push(Line::default());
                        }
                    } else {
                        let style = Style::default();
                        if e.text.is_empty() {
                            display_lines.push(Line::from(Span::styled("", style)));
                        } else {
                            for sub in e.text.split('\n') {
                                display_lines
                                    .push(Line::from(Span::styled(sub.to_string(), style)));
                            }
                        }
                    }
                }
                let line_count = display_lines.len();
                let area_height = activity_log.height as usize;
                let scroll_y = if line_count > area_height {
                    let max_scroll = line_count.saturating_sub(area_height);
                    (view_state.scroll_offset.min(max_scroll)) as u16
                } else {
                    0
                };
                let widget = Paragraph::new(Text::from(display_lines)).scroll((scroll_y, 0));
                frame.render_widget(widget, activity_log);
                if matches!(state.mode, AppMode::Running) && !view_state.running_input.is_empty() {
                    paint_user_prompt_activity_strip(
                        frame,
                        activity_log,
                        &view_state.running_input,
                    );
                }
            }
        }
    }

    if dynamic_area.height > 0 {
        match &state.mode {
            AppMode::FeatureInput if view_state.feature_slash_open => {
                render_feature_slash_menu(frame, view_state, dynamic_area);
            }
            AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. } => {
                render_question(frame, state, view_state, dynamic_area);
            }
            AppMode::ErrorRecovery { .. } => {
                render_error_recovery(frame, state, view_state, dynamic_area);
            }
            _ => {
                render_inbox(frame, state, view_state, dynamic_area);
            }
        }
    }

    if debug_log.height > 0 && !debug_logs.is_empty() {
        use ratatui::style::Style;
        let content = debug_logs
            .iter()
            .rev()
            .take(debug_log.height as usize)
            .rev()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let widget =
            Paragraph::new(content).style(Style::default().fg(ratatui::style::Color::DarkGray));
        frame.render_widget(widget, debug_log);
    }

    if status_bar.height > 0 {
        let text = status_bar_text_for_draw(state, view_state);
        let style = status_bar_style_for_goal(state.current_goal.as_deref());
        let widget = Paragraph::new(text).style(style);
        frame.render_widget(widget, status_bar);
    }

    if prompt_bar.height > 0 {
        let text_area = if prompt_bar.height > 1 {
            Rect::new(
                prompt_bar.x,
                prompt_bar.y,
                prompt_bar.width,
                prompt_bar.height.saturating_sub(1),
            )
        } else {
            prompt_bar
        };
        // Split by characters (not words) so wrapping matches [`crate::layout::prompt_height`]
        // over the **content** width (prompt chunk minus rule row). Word-wrapping puts a short
        // prefix like "> " alone on its own line when followed by a long single-word payload,
        // causing the last partial row to overflow the allocated height and be clipped.
        let w = text_area.width as usize;
        let lines: Vec<Line> = if matches!(state.mode, AppMode::FeatureInput) {
            let skill_style = feature_skill_token_style();
            let mut spans: Vec<Span<'static>> = vec![Span::raw("> ")];
            if view_state.feature_slash_open {
                spans.push(Span::raw(format!(
                    "Slash menu  Up/Down  Enter apply  Esc cancel — {}",
                    view_state.feature_edit.display()
                )));
            } else if view_state.feature_edit.display().is_empty() {
                spans.push(Span::raw(
                    "Type your feature description and press Enter...",
                ));
            } else {
                for seg in view_state.feature_edit.prompt_segments() {
                    match seg {
                        FeaturePromptSegment::Plain(t) => spans.push(Span::raw(t)),
                        FeaturePromptSegment::SkillName(name) => {
                            spans.push(Span::styled(format!("/{name}"), skill_style));
                        }
                    }
                }
            }
            wrap_spans_to_prompt_lines(spans, w)
        } else if w == 0 {
            vec![Line::raw(prompt_text_str.as_str())]
        } else {
            prompt_text_str
                .chars()
                .collect::<Vec<_>>()
                .chunks(w)
                .map(|chunk| Line::raw(chunk.iter().collect::<String>()))
                .collect()
        };
        let widget = Paragraph::new(lines);
        frame.render_widget(widget, text_area);
        if prompt_bar.height > 1 {
            paint_prompt_bottom_horizontal_rule(frame, prompt_bar);
        }
        if let Some(pos) = editing_prompt_cursor_position(state, view_state, text_area) {
            frame.set_cursor_position(pos);
        }
    }

    if footer_bar.height > 0 {
        log::debug!("draw: footer_bar row rect={footer_bar:?}");
        let widget = Paragraph::new("");
        frame.render_widget(widget, footer_bar);
    }

    paint_enter_affordance(
        frame,
        &LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: enter_hit,
            stop_pane: stop_hit,
        },
    );
    paint_stop_affordance(
        frame,
        &LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: enter_hit,
            stop_pane: stop_hit,
        },
    );

    // Advance fast spinner phase only while agent-active (Running). Clarification wait uses a 1 Hz
    // idle dot driven by wall time, not `spinner_tick`.
    if area.width >= 2 && area.height >= 1 && status_activity_is_agent_active(&state.mode) {
        view_state.spinner_tick = view_state.spinner_tick.wrapping_add(1);
    }
}

/// Feature-prompt slash menu: `/recipe` plus discovered `.agents/skills` entries.
fn render_feature_slash_menu(frame: &mut Frame, view_state: &ViewState, area: Rect) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use tddy_core::SlashMenuEntry;

    if area.height == 0 {
        return;
    }
    let mut lines = vec![Line::from(Span::styled(
        "Commands & skills (.agents/skills/)",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let max_rows = (area.height.saturating_sub(2)) as usize;
    let cap = max_rows.max(1);
    for (i, entry) in view_state
        .feature_slash_entries
        .iter()
        .take(cap)
        .enumerate()
    {
        let prefix = if i == view_state.feature_slash_selected {
            "> "
        } else {
            "  "
        };
        let (label, desc) = match entry {
            SlashMenuEntry::BuiltinRecipe => (
                "/recipe".to_string(),
                "Switch workflow recipe (TDD / bugfix)".to_string(),
            ),
            SlashMenuEntry::StartRecipe { label: start_label } => (
                start_label.clone(),
                "Start structured workflow for this session".to_string(),
            ),
            SlashMenuEntry::Skill { name, description } => {
                (format!("/{name}"), description.clone())
            }
        };
        let text = if desc.is_empty() {
            format!("{prefix}{label}")
        } else {
            format!("{prefix}{label} — {desc}")
        };
        let style = if i == view_state.feature_slash_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::from(Span::raw("Enter select  Up/Down  Esc cancel")));
    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render clarification question (Select, MultiSelect, or TextInput mode).
fn render_question(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let lines: Vec<Line> = match &state.mode {
        AppMode::Select {
            question,
            question_index,
            total_questions: _,
            initial_selected: _,
        } => {
            let header = format!(
                "[{}] {}: {}",
                question_index + 1,
                question.header,
                question.question
            );
            let mut result = vec![Line::from(Span::styled(
                header,
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let other_idx = question.options.len();
            for (i, opt) in question.options.iter().enumerate() {
                let prefix = if view_state.select_selected == i {
                    "> "
                } else {
                    "  "
                };
                let text = if opt.description.is_empty() {
                    format!("{}{}", prefix, opt.label)
                } else {
                    format!("{}{} -- {}", prefix, opt.label, opt.description)
                };
                let style = if view_state.select_selected == i {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(text, style)));
            }
            if question.allow_other {
                let other_prefix = if view_state.select_selected == other_idx {
                    "> "
                } else {
                    "  "
                };
                let other_text = if view_state.select_typing_other {
                    format!("{}Other: {}", other_prefix, view_state.select_other_text)
                } else {
                    format!("{}Other (type your own)", other_prefix)
                };
                let other_style = if view_state.select_selected == other_idx {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(other_text, other_style)));
            }
            result
        }
        AppMode::MultiSelect {
            question,
            question_index,
            total_questions: _,
        } => {
            let header = format!(
                "[{}] {}: {}",
                question_index + 1,
                question.header,
                question.question
            );
            let mut result = vec![Line::from(Span::styled(
                header,
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let other_idx = question.options.len();
            for (i, opt) in question.options.iter().enumerate() {
                let checked = view_state
                    .multiselect_checked
                    .get(i)
                    .copied()
                    .unwrap_or(false);
                let checkbox = if checked { "[x]" } else { "[ ]" };
                let prefix = if view_state.multiselect_cursor == i {
                    "> "
                } else {
                    "  "
                };
                let text = if opt.description.is_empty() {
                    format!("{}{} {}", prefix, checkbox, opt.label)
                } else {
                    format!(
                        "{}{} {} -- {}",
                        prefix, checkbox, opt.label, opt.description
                    )
                };
                let style = if view_state.multiselect_cursor == i {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(text, style)));
            }
            if question.allow_other {
                let other_checked = view_state
                    .multiselect_checked
                    .get(other_idx)
                    .copied()
                    .unwrap_or(false);
                let other_cb = if other_checked { "[x]" } else { "[ ]" };
                let other_prefix = if view_state.multiselect_cursor == other_idx {
                    "> "
                } else {
                    "  "
                };
                let other_text = if view_state.multiselect_typing_other {
                    format!(
                        "{}{} Other: {}",
                        other_prefix, other_cb, view_state.multiselect_other_text
                    )
                } else {
                    format!("{}{} Other (type your own)", other_prefix, other_cb)
                };
                let other_style = if view_state.multiselect_cursor == other_idx {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(other_text, other_style)));
            }
            result
        }
        AppMode::TextInput { prompt } => {
            vec![
                Line::from(Span::styled(
                    prompt.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("")),
            ]
        }
        _ => return,
    };

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render plan approval 3-option menu
fn render_document_review(
    frame: &mut Frame,
    _state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let options = [
        ("View", "Open full-screen PRD viewer"),
        ("Approve", "Proceed to next step"),
        ("Refine", "Enter feedback for plan refinement"),
    ];
    let mut lines = vec![Line::from(Span::styled(
        "Plan generated. Choose an action:",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for (i, (label, desc)) in options.iter().enumerate() {
        let prefix = if view_state.document_review_selected == i {
            "> "
        } else {
            "  "
        };
        let text = format!("{}{} -- {}", prefix, label, desc);
        let style = if view_state.document_review_selected == i {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render error recovery: error message + 3 selectable options.
fn render_error_recovery(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let error_msg = match &state.mode {
        AppMode::ErrorRecovery { error_message } => error_message.as_str(),
        _ => return,
    };

    let options = ["Resume", "Continue with agent", "Exit"];
    let highlight = Style::default().add_modifier(Modifier::REVERSED);
    let normal = Style::default();

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Error: {}", error_msg),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, label) in options.iter().enumerate() {
        let selected = view_state.error_recovery_selected == i;
        let prefix = if selected { "> " } else { "  " };
        let style = if selected { highlight } else { normal };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, label),
            style,
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Render inbox items.
fn render_inbox(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 || state.inbox.is_empty() {
        return;
    }

    let highlight_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default();

    let lines: Vec<Line> = state
        .inbox
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = matches!(
                view_state.inbox_focus,
                InboxFocus::List | InboxFocus::Editing
            ) && i == view_state.inbox_cursor;
            let display_text = if is_selected && view_state.inbox_focus == InboxFocus::Editing {
                &view_state.inbox_edit_buffer
            } else {
                item
            };
            let text = format!("[{}] {}", i + 1, display_text);
            let style = if is_selected {
                highlight_style
            } else {
                normal_style
            };
            Line::from(Span::styled(text, style))
        })
        .collect();

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;
    use tddy_core::{
        ActivityEntry, ActivityKind, AppMode, ClarificationQuestion, PresenterState, QuestionOption,
    };

    use crate::mouse_map::{enter_button_rect, stop_button_rect, LayoutAreas};
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Position, Rect};
    use ratatui::style::Color;

    static ENTER_AFFORDANCE_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn status_bar_row_compact_prefix(buffer: &Buffer, area: Rect) -> String {
        let y = area.y;
        (area.x..area.x.saturating_add(area.width))
            .filter_map(|col| {
                buffer
                    .cell(Position::new(col, y))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn user_prompt_activity_row_strings_keeps_first_row_empty() {
        let r = user_prompt_activity_row_strings("hello", 80);
        assert_eq!(r[0], "");
        assert_eq!(r[1], "hello");
        assert_eq!(r[2], "");
    }

    #[test]
    fn user_prompt_activity_row_strings_ellipsis_on_overflow() {
        let s: String = std::iter::repeat_n('a', 50).collect();
        let r = user_prompt_activity_row_strings(&s, 10);
        assert_eq!(r[0], "");
        assert_eq!(r[1].chars().count(), 10);
        assert!(r[2].ends_with('…'), "got {:?}", r[2]);
    }

    /// Acceptance (PRD): Virtual TUI uses the same `draw()` path as the local TUI; autonomous
    /// periodic re-renders must advance the spinner and elapsed timer without freezing. After
    /// relocation, the cycling spinner frame must be the leading content of the status bar row
    /// (before `Goal:`), not only a detached top-right cell.
    #[test]
    fn virtual_tui_still_emits_bytes_while_idle() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = PresenterState {
            agent: "agent".to_string(),
            model: "model".to_string(),
            mode: AppMode::Running,
            current_goal: Some("plan".to_string()),
            current_state: Some("Running".to_string()),
            workflow_session_id: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
            skills_project_root: None,
            active_worktree_display: None,
        };
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let line = status_bar_row_compact_prefix(buf, areas.status_bar);
        let first = line.chars().next();
        assert!(
            first.is_some_and(|c| super::SPINNER_FRAMES.contains(&c)),
            "status bar must lead with a spinner frame before Goal: (shared with Virtual TUI); \
             first char={first:?} line={line:?}"
        );
        let goal_pos = line.find("Goal:").expect("Goal: in status bar");
        let spin_pos = line
            .chars()
            .enumerate()
            .find(|(_, c)| super::SPINNER_FRAMES.contains(c))
            .map(|(i, _)| i)
            .expect("spinner in line");
        assert!(
            spin_pos < goal_pos,
            "spinner must appear before Goal: in status line, got {line:?}"
        );
    }

    fn make_state(mode: AppMode) -> PresenterState {
        PresenterState {
            agent: "test-agent".to_string(),
            model: "test-model".to_string(),
            mode,
            current_goal: None,
            current_state: None,
            workflow_session_id: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
            skills_project_root: None,
            active_worktree_display: None,
        }
    }

    /// Third pipe-separated segment in the `… │ … │ …` status tail (elapsed like `3s`, or `Ready` in idle).
    fn elapsed_segment_from_goal_status_line(line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split(" │ ").collect();
        if parts.len() >= 3 {
            Some(parts[2].to_string())
        } else {
            None
        }
    }

    fn first_activity_char(line: &str) -> Option<char> {
        line.chars().next()
    }

    /// PRD: idle clarification wait uses heartbeat glyphs (not `SPINNER_FRAMES`).
    const IDLE_DOT_PULSE_CHARS: &[char] = &['·', '•', '●'];

    fn select_mode_with_goal() -> AppMode {
        AppMode::Select {
            question: ClarificationQuestion {
                header: "Scope".to_string(),
                question: "Pick one".to_string(),
                options: vec![QuestionOption {
                    label: "A".to_string(),
                    description: String::new(),
                }],
                multi_select: false,
                allow_other: false,
            },
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        }
    }

    /// PRD AC1: user-submitted prompt appears in the activity pane as white on dark grey.
    #[test]
    fn activity_buffer_user_prompt_strip_white_on_grey() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut state = make_state(AppMode::Running);
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        state.activity_log = vec![ActivityEntry {
            text: "agent activity line".to_string(),
            kind: ActivityKind::AgentOutput,
        }];
        let mut vs = ViewState::new();
        vs.running_input = "USER_PROMPT_STRIP".to_string();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let strip_y = areas.activity_log.y + areas.activity_log.height - 1;
        let label = "> USER_PROMPT_STRIP";
        for (i, _) in label.chars().enumerate() {
            let x = areas.activity_log.x + i as u16;
            let cell = buf
                .cell(Position::new(x, strip_y))
                .expect("user prompt strip cell");
            let st = cell.style();
            assert_eq!(
                st.fg,
                Some(Color::White),
                "user prompt strip fg at x={x} strip_y={strip_y}"
            );
            assert_eq!(
                st.bg,
                Some(Color::DarkGray),
                "user prompt strip bg at x={x} strip_y={strip_y}"
            );
        }
    }

    /// PRD AC4 / FR5: env gate skips overlay; layout exposes footer for full-height affordance.
    #[test]
    #[allow(non_snake_case)]
    fn TDDY_E2E_NO_ENTER_AFFORDANCE_skips_enter_overlay_paint() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let _lock = ENTER_AFFORDANCE_ENV_LOCK.lock().expect("env lock");
        std::env::set_var("TDDY_E2E_NO_ENTER_AFFORDANCE", "1");

        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        std::env::remove_var("TDDY_E2E_NO_ENTER_AFFORDANCE");

        assert_eq!(
            areas.footer_bar.height,
            1,
            "draw must allocate exactly one footer row (PRD: +1 row; enter_button_rect/paint parity)"
        );

        let r = enter_button_rect(&areas);
        let buf = terminal.backend().buffer();
        const RETURN_SYMBOL: &str = "\u{23CE}";
        const STOP_SYMBOL: &str = "\u{25A0}";
        for y in r.y..r.y.saturating_add(r.height) {
            for x in r.x..r.x.saturating_add(r.width) {
                let sym = buf
                    .cell(Position::new(x, y))
                    .map(|c| c.symbol())
                    .unwrap_or_default();
                assert!(
                    sym != "+"
                        && sym != "|"
                        && !sym.contains(RETURN_SYMBOL)
                        && !sym.contains(STOP_SYMBOL)
                        && !(sym == "-" && x >= r.x && x < r.x + r.width),
                    "Enter overlay symbols must not be written when TDDY_E2E_NO_ENTER_AFFORDANCE is set; got {sym:?} at ({x},{y})"
                );
            }
        }
        let sr = stop_button_rect(&areas);
        for y in sr.y..sr.y.saturating_add(sr.height) {
            for x in sr.x..sr.x.saturating_add(sr.width) {
                let sym = buf
                    .cell(Position::new(x, y))
                    .map(|c| c.symbol())
                    .unwrap_or_default();
                assert!(
                    !sym.contains(RETURN_SYMBOL) && !sym.contains(STOP_SYMBOL),
                    "Stop overlay symbols must not be written when TDDY_E2E_NO_ENTER_AFFORDANCE is set; got {sym:?} at ({x},{y})"
                );
            }
        }
    }

    /// After `draw`, `LayoutAreas.enter_pane` matches the pointer Enter hit target (dedicated strip).
    #[test]
    fn layout_enter_pane_matches_enter_button_rect_after_draw() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let hit = enter_button_rect(&areas);
        assert_eq!(
            areas.enter_pane, hit,
            "enter_pane must equal enter_button_rect for hit-test and paint alignment"
        );
        assert!(areas.enter_pane.width > 0, "enter_pane must be allocated");
        let stop_hit = stop_button_rect(&areas);
        assert_eq!(
            areas.stop_pane, stop_hit,
            "stop_pane must equal stop_button_rect for hit-test and paint alignment"
        );
        assert!(
            areas.stop_pane.width > 0,
            "stop_pane must be allocated on wide terminal"
        );
    }

    /// Bottom row of the prompt pane is filled with U+2500 light horizontal box-drawing characters.
    #[test]
    fn prompt_pane_bottom_row_is_horizontal_rule_line() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let y = areas.prompt_bar.y + areas.prompt_bar.height - 1;
        let rule = '\u{2500}';
        for x in areas.prompt_bar.x..areas.prompt_bar.x + areas.prompt_bar.width {
            let sym = buf
                .cell(Position::new(x, y))
                .map(|c| c.symbol().chars().next().unwrap_or(' '))
                .unwrap_or(' ');
            assert_eq!(
                sym, rule,
                "expected horizontal rule at bottom of prompt pane at x={x} y={y}"
            );
        }
    }

    /// Acceptance (PRD): `status_bar_frozen_elapsed_in_select_mode` — in Select with an active goal,
    /// the compact elapsed token in the status line must not advance across wall time while the
    /// mode is unchanged (clock frozen during clarification wait).
    #[test]
    fn status_bar_frozen_elapsed_in_select_mode() {
        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        state.goal_start_time = Instant::now();
        let mut vs = ViewState::new();

        let before = status_bar_text_for_draw(&state, &mut vs);
        let e0 = elapsed_segment_from_goal_status_line(&before).expect("elapsed segment");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        vs.spinner_tick = vs.spinner_tick.wrapping_add(400);
        let after = status_bar_text_for_draw(&state, &mut vs);
        let e1 = elapsed_segment_from_goal_status_line(&after).expect("elapsed segment");

        assert_eq!(
            e0, e1,
            "PRD: elapsed display must stay fixed in Select while waiting; before={before:?} after={after:?}"
        );
    }

    /// Acceptance (PRD): `status_bar_idle_dot_not_spinner_in_text_input` — TextInput wait must use
    /// the idle dot pulse (`·` / `•`), not characters from `SPINNER_FRAMES`.
    #[test]
    fn status_bar_idle_dot_not_spinner_in_text_input() {
        let mut state = make_state(AppMode::TextInput {
            prompt: "Why?".to_string(),
        });
        state.current_goal = Some("acceptance-tests".to_string());
        state.current_state = Some("Running".to_string());
        state.goal_start_time = Instant::now();
        let mut vs = ViewState::new();

        let line = status_bar_text_for_draw(&state, &mut vs);
        let lead = first_activity_char(&line).expect("leading char");
        assert!(
            !super::SPINNER_FRAMES.contains(&lead),
            "expected idle dot prefix, not spinner frame {lead:?} in {line:?}"
        );
        assert!(
            IDLE_DOT_PULSE_CHARS.contains(&lead),
            "expected idle pulse glyph (· or •), got {lead:?} in {line:?}"
        );
    }

    /// Acceptance (PRD): `status_bar_running_mode_uses_spinner_and_live_elapsed` — Running keeps a
    /// fast spinner and a live elapsed clock; user-wait modes must not cycle `SPINNER_FRAMES` on
    /// tick advances (idle dot only).
    #[test]
    fn status_bar_running_mode_uses_spinner_and_live_elapsed() {
        let start = Instant::now();
        let mut running = make_state(AppMode::Running);
        running.current_goal = Some("plan".to_string());
        running.current_state = Some("Running".to_string());
        running.goal_start_time = start;
        let mut vs_run = ViewState::new();
        let line_tick0 = status_bar_text_for_draw(&running, &mut vs_run);
        vs_run.spinner_tick = 4;
        let line_tick4 = status_bar_text_for_draw(&running, &mut vs_run);
        let c0 = first_activity_char(&line_tick0).unwrap();
        let c4 = first_activity_char(&line_tick4).unwrap();
        assert!(
            super::SPINNER_FRAMES.contains(&c0) && super::SPINNER_FRAMES.contains(&c4),
            "Running mode should use spinner frames; tick0={c0:?} tick4={c4:?}"
        );
        assert_ne!(
            c0, c4,
            "Running spinner should advance between tick 0 and 4; lines {line_tick0:?} {line_tick4:?}"
        );

        std::thread::sleep(std::time::Duration::from_millis(1100));
        let line_late = status_bar_text_for_draw(&running, &mut vs_run);
        let e_early = elapsed_segment_from_goal_status_line(&line_tick0).unwrap();
        let e_late = elapsed_segment_from_goal_status_line(&line_late).unwrap();
        assert_ne!(
            e_early, e_late,
            "Running elapsed display should advance over wall time; early={e_early} late={e_late}"
        );

        let mut state_sel = make_state(select_mode_with_goal());
        state_sel.current_goal = Some("plan".to_string());
        state_sel.current_state = Some("Running".to_string());
        state_sel.goal_start_time = start;
        let mut vs_sel = ViewState::new();
        let s0 = first_activity_char(&status_bar_text_for_draw(&state_sel, &mut vs_sel)).unwrap();
        vs_sel.spinner_tick = 4;
        let s4 = first_activity_char(&status_bar_text_for_draw(&state_sel, &mut vs_sel)).unwrap();
        assert!(
            IDLE_DOT_PULSE_CHARS.contains(&s0) && IDLE_DOT_PULSE_CHARS.contains(&s4),
            "Select wait must use idle dot glyphs only, not spinner; s0={s0:?} s4={s4:?}"
        );
        assert_eq!(
            s0, s4,
            "Select idle prefix should not advance on fast ticks (1 Hz pulse); s0={s0:?} s4={s4:?}"
        );
    }

    /// Lower-level (PRD): worktree display token is woven into the status line string.
    #[test]
    fn inject_worktree_into_status_line_inserts_display_token() {
        let out = super::inject_worktree_into_status_line(
            "│ prefix │ Goal: x".to_string(),
            Some("wt-acceptance-marker"),
        );
        assert!(
            out.contains("wt-acceptance-marker"),
            "expected worktree token in status line; out={out:?}"
        );
    }

    /// Lower-level (PRD): editing FeatureInput must yield a terminal cursor anchor for the insert index.
    #[test]
    fn editing_prompt_cursor_position_some_for_feature_input() {
        let state = make_state(AppMode::FeatureInput);
        let mut vs = ViewState::new();
        vs.feature_edit.set_plain_text("hello");
        let pos = super::editing_prompt_cursor_position(&state, &vs, Rect::new(0, 20, 80, 2));
        assert!(
            pos.is_some(),
            "expected hardware cursor position for FeatureInput editing"
        );
    }

    /// Lower-level (PRD): at end-of-scroll the layout strategy must embed Approve/Reject in the document tail.
    #[test]
    fn markdown_plan_action_layout_uses_tail_when_at_end() {
        assert_eq!(
            super::markdown_plan_action_layout_for_view(true),
            super::MarkdownPlanActionLayout::TailWithDocumentWhenAtEnd,
        );
    }

    /// Lower-level (PRD): mid-scroll must not keep the permanent fixed Approve/Reject footer mode.
    #[test]
    fn markdown_plan_action_layout_avoids_fixed_footer_mid_scroll() {
        assert_ne!(
            super::markdown_plan_action_layout_for_view(false),
            super::MarkdownPlanActionLayout::FixedFooterAlways,
        );
    }

    /// Acceptance (PRD): status line includes the active worktree display segment when
    /// [`PresenterState::active_worktree_display`] is set.
    #[test]
    fn status_bar_includes_worktree_path_when_present() {
        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        state.goal_start_time = Instant::now();
        state.active_worktree_display = Some("wt-acceptance-marker/path".to_string());
        let mut vs = ViewState::new();
        let line = status_bar_text_for_draw(&state, &mut vs);
        assert!(
            line.contains("wt-acceptance-marker"),
            "status bar must include presenter worktree display when set; line={line:?}"
        );
    }

    /// Acceptance (PRD): hardware cursor sits at the UTF-8 insert index for FeatureInput prompts.
    #[test]
    fn prompt_cursor_position_matches_utf8_feature_input() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::FeatureInput);
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };

        vs.feature_edit.set_plain_text("abcdef");
        vs.feature_edit.cursor = 3;
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        let prompt_ascii = format!("> {}", vs.feature_edit.display());
        let expected_ascii = super::terminal_position_for_byte_cursor_in_char_wrapped_prompt(
            &prompt_ascii,
            "> ".len() + vs.feature_edit.cursor,
            areas.prompt_bar,
        )
        .expect("ascii cursor");
        assert_eq!(
            terminal.get_cursor_position().expect("cursor ascii"),
            expected_ascii,
            "ASCII insert position"
        );

        vs.feature_edit.set_plain_text("a🙂b");
        vs.feature_edit.cursor = 1 + '🙂'.len_utf8();
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        let prompt_utf8 = format!("> {}", vs.feature_edit.display());
        let expected_utf8 = super::terminal_position_for_byte_cursor_in_char_wrapped_prompt(
            &prompt_utf8,
            "> ".len() + vs.feature_edit.cursor,
            areas.prompt_bar,
        )
        .expect("utf8 cursor");
        assert_eq!(
            terminal.get_cursor_position().expect("cursor utf8"),
            expected_utf8,
            "multi-byte insert position"
        );
    }

    #[test]
    fn test_error_recovery_prompt_text() {
        let state = make_state(AppMode::ErrorRecovery {
            error_message: "timeout".to_string(),
        });
        let vs = ViewState::new();
        let text = prompt_text(&state, &vs);
        assert_eq!(text, "Up/Down navigate  Enter select");
    }

    #[test]
    fn test_render_error_recovery_shows_three_options() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = make_state(AppMode::ErrorRecovery {
            error_message: "backend timeout".to_string(),
        });
        let vs = ViewState::new(); // error_recovery_selected defaults to 0

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_error_recovery(frame, &state, &vs, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(
            content.contains("Resume"),
            "Error recovery should render 'Resume' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("Continue with agent"),
            "Error recovery should render 'Continue with agent' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("Exit"),
            "Error recovery should render 'Exit' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("backend timeout"),
            "Error recovery should render the error message, got: {}",
            content.trim()
        );
    }

    /// PRD: While the PRD is visible in the activity pane, the prompt bar tells the user they can
    /// type refinement feedback (e.g. after Reject or when the refine affordance is focused).
    #[test]
    fn markdown_viewer_prompt_shows_refinement_hint_when_reject_or_focused() {
        let state = make_state(AppMode::MarkdownViewer {
            content: "# My PRD".to_string(),
        });
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 1;
        let text = prompt_text(&state, &vs);
        let lower = text.to_lowercase();
        assert!(
            lower.contains("feedback") && text.contains("Enter"),
            "prompt must describe typing refinement feedback and submission while PRD remains visible; got {:?}",
            text
        );
    }

    #[test]
    fn plan_review_frame_shows_view_approve_refine_menu() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::DocumentReview {
            content: "# Plan".to_string(),
        });
        let mut vs = ViewState::new();
        vs.document_review_selected = 1;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            content.contains("Plan generated. Choose an action:"),
            "expected plan approval menu header in rendered frame"
        );
        assert!(
            content.contains("View") && content.contains("Approve") && content.contains("Refine"),
            "expected View, Approve, and Refine labels from the plan approval menu in the frame"
        );
    }

    #[test]
    fn markdown_viewer_draw_marks_at_end_when_scrolled_to_bottom() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# Long PRD\n\n".to_string() + &"line\n".repeat(200),
        });
        let mut vs = ViewState::new();
        vs.markdown_scroll_offset = 50_000;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(
            vs.markdown_at_end,
            "scrolling to the document end must enable footer Approve/Reject navigation"
        );
    }

    /// Markdown viewer body rect: full activity region (plan actions scroll as document tail).
    fn markdown_viewer_body_rect(activity: Rect) -> Rect {
        activity
    }

    fn rect_plaintext(buffer: &Buffer, area: Rect) -> String {
        let mut s = String::new();
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                let ch = buffer
                    .cell(Position::new(x, y))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
                    .unwrap_or(' ');
                s.push(ch);
            }
        }
        s
    }

    /// Acceptance (PRD): before end-of-scroll, the plan viewer must not show Approve and Reject together.
    #[test]
    fn markdown_viewer_hides_approve_reject_until_scrolled_to_end() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# Tall plan\n\n".to_string() + &"paragraph line\n".repeat(120),
        });
        let mut vs = ViewState::new();
        vs.markdown_scroll_offset = 0;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(
            !vs.markdown_at_end,
            "fixture expects mid-document scroll (not at end)"
        );
        let buffer = terminal.backend().buffer();
        let activity = rect_plaintext(buffer, areas.activity_log);
        assert!(
            !(activity.contains("Approve") && activity.contains("Reject")),
            "mid-scroll plan view must not show both Approve and Reject; activity={activity:?}"
        );
    }

    /// Acceptance (PRD): at max scroll, Approve and Reject appear as trailing scroll content (inside the markdown body), not only a reserved footer band.
    #[test]
    fn markdown_viewer_shows_approve_reject_at_document_tail_after_end() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# Tall plan\n\n".to_string() + &"paragraph line\n".repeat(120),
        });
        let mut vs = ViewState::new();
        vs.markdown_scroll_offset = usize::MAX;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
            footer_bar: Rect::default(),
            enter_pane: Rect::default(),
            stop_pane: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(vs.markdown_at_end, "fixture expects end-of-document scroll");
        let buffer = terminal.backend().buffer();
        let body = markdown_viewer_body_rect(areas.activity_log);
        let body_text = rect_plaintext(buffer, body);
        assert!(
            body_text.contains("Approve") && body_text.contains("Reject"),
            "Approve and Reject must render inside the scrollable markdown region at end-of-doc; body={body_text:?}"
        );
    }
}
