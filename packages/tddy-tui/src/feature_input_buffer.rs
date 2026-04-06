//! Feature prompt buffer: display uses compact `/skill-name` tokens; submit expands for the agent.

use tddy_core::{compose_prompt_skill_reference, AGENTS_SKILLS_DIR};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureInputChunk {
    Text(String),
    SkillToken { name: String },
}

/// Display segment for styled feature-prompt rendering (UI adds `/` before [`Self::SkillName`]).
#[derive(Debug, Clone)]
pub enum FeaturePromptSegment {
    Plain(String),
    SkillName(String),
}

/// Editing buffer for [`super::ViewState`] feature input.
#[derive(Debug, Clone)]
pub struct FeatureInputBuffer {
    /// Starts and ends with [`FeatureInputChunk::Text`] (possibly empty).
    chunks: Vec<FeatureInputChunk>,
    /// Byte index in [`Self::display`]; always on a token boundary (never inside `/name`).
    pub cursor: usize,
}

impl Default for FeatureInputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureInputBuffer {
    pub fn new() -> Self {
        Self {
            chunks: vec![FeatureInputChunk::Text(String::new())],
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.chunks = vec![FeatureInputChunk::Text(String::new())];
        self.cursor = 0;
    }

    /// Plain text only (tests and simple cases).
    pub fn set_plain_text(&mut self, s: &str) {
        self.chunks = vec![FeatureInputChunk::Text(s.to_string())];
        self.cursor = s.len();
    }

    pub fn display(&self) -> String {
        let mut out = String::new();
        for c in &self.chunks {
            match c {
                FeatureInputChunk::Text(t) => out.push_str(t),
                FeatureInputChunk::SkillToken { name } => {
                    out.push('/');
                    out.push_str(name);
                }
            }
        }
        out
    }

    /// Segments for ratatui styling (slash menu / placeholder callers use plain strings instead).
    pub fn prompt_segments(&self) -> Vec<FeaturePromptSegment> {
        let mut v = Vec::new();
        for c in &self.chunks {
            match c {
                FeatureInputChunk::Text(t) if !t.is_empty() => {
                    v.push(FeaturePromptSegment::Plain(t.clone()));
                }
                FeatureInputChunk::Text(_) => {}
                FeatureInputChunk::SkillToken { name } => {
                    v.push(FeaturePromptSegment::SkillName(name.clone()));
                }
            }
        }
        v
    }

    /// True if there is nothing to submit (ignores leading/trailing whitespace).
    pub fn is_submit_empty(&self) -> bool {
        self.display().trim().is_empty()
    }

    /// Outbound text for [`tddy_core::UserIntent::SubmitFeatureInput`].
    pub fn submit_expanded(&self) -> String {
        let mut out = String::new();
        let mut text_buf = String::new();
        for c in &self.chunks {
            match c {
                FeatureInputChunk::Text(t) => text_buf.push_str(t),
                FeatureInputChunk::SkillToken { name } => {
                    let rel = format!("{}/{}/SKILL.md", AGENTS_SKILLS_DIR, name);
                    let block = compose_prompt_skill_reference(name, &rel, text_buf.trim_end());
                    if !out.is_empty() {
                        out.push_str("\n\n");
                    }
                    out.push_str(&block);
                    text_buf.clear();
                }
            }
        }
        let tail = text_buf.trim();
        if !tail.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(tail);
        }
        out
    }

    fn merge_adjacent_text(&mut self) {
        let mut merged: Vec<FeatureInputChunk> = Vec::new();
        for ch in std::mem::take(&mut self.chunks) {
            match ch {
                FeatureInputChunk::Text(s) => {
                    if let Some(FeatureInputChunk::Text(prev)) = merged.last_mut() {
                        prev.push_str(&s);
                    } else {
                        merged.push(FeatureInputChunk::Text(s));
                    }
                }
                FeatureInputChunk::SkillToken { name } => {
                    merged.push(FeatureInputChunk::SkillToken { name });
                }
            }
        }
        if merged.is_empty() {
            merged.push(FeatureInputChunk::Text(String::new()));
        } else if !matches!(merged.first(), Some(FeatureInputChunk::Text(_))) {
            merged.insert(0, FeatureInputChunk::Text(String::new()));
        }
        if !matches!(merged.last(), Some(FeatureInputChunk::Text(_))) {
            merged.push(FeatureInputChunk::Text(String::new()));
        }
        self.chunks = merged;
    }

    fn snap_cursor(&mut self) {
        let d = self.display();
        if self.cursor > d.len() {
            self.cursor = d.len();
        }
        let mut flat = 0usize;
        for ch in &self.chunks {
            match ch {
                FeatureInputChunk::Text(t) => {
                    flat += t.len();
                }
                FeatureInputChunk::SkillToken { name } => {
                    let slen = 1 + name.len();
                    if self.cursor > flat && self.cursor < flat + slen {
                        self.cursor = flat;
                    }
                    flat += slen;
                }
            }
        }
    }

    fn flat_after_skill_chunk(&self, skill_chunk_idx: usize) -> Option<usize> {
        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            if i == skill_chunk_idx {
                return match ch {
                    FeatureInputChunk::SkillToken { name } => Some(flat + 1 + name.len()),
                    _ => None,
                };
            }
            match ch {
                FeatureInputChunk::Text(t) => flat += t.len(),
                FeatureInputChunk::SkillToken { name } => flat += 1 + name.len(),
            }
        }
        None
    }

    pub fn insert_char(&mut self, c: char) {
        let d = self.display();
        let len = d.len();
        if self.cursor > len {
            self.cursor = len;
        }

        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            match ch {
                FeatureInputChunk::Text(t) => {
                    let tlen = t.len();
                    let end = flat + tlen;
                    if self.cursor >= flat && self.cursor <= end {
                        let off = self.cursor - flat;
                        if let FeatureInputChunk::Text(t_mut) = &mut self.chunks[i] {
                            t_mut.insert(off, c);
                            self.cursor += c.len_utf8();
                        }
                        self.merge_adjacent_text();
                        self.snap_cursor();
                        return;
                    }
                    flat = end;
                }
                FeatureInputChunk::SkillToken { name } => {
                    let slen = 1 + name.len();
                    let skill_start = flat;
                    let skill_end = flat + slen;
                    if self.cursor == skill_start {
                        if let FeatureInputChunk::Text(prev) = &mut self.chunks[i - 1] {
                            prev.push(c);
                            self.cursor = skill_start + c.len_utf8();
                        }
                        self.merge_adjacent_text();
                        self.snap_cursor();
                        return;
                    }
                    if self.cursor == skill_end {
                        if let FeatureInputChunk::Text(next) = &mut self.chunks[i + 1] {
                            next.insert(0, c);
                            self.cursor = skill_end + c.len_utf8();
                        }
                        self.merge_adjacent_text();
                        self.snap_cursor();
                        return;
                    }
                    flat = skill_end;
                }
            }
        }
        if let Some(FeatureInputChunk::Text(t)) = self.chunks.last_mut() {
            t.push(c);
            self.cursor += c.len_utf8();
        }
        self.merge_adjacent_text();
        self.snap_cursor();
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            match ch {
                FeatureInputChunk::Text(t) => {
                    let tlen = t.len();
                    let end = flat + tlen;
                    if self.cursor > flat && self.cursor <= end {
                        let off = self.cursor - flat;
                        if off > 0 {
                            let prev = prev_char_boundary(t, off);
                            if let FeatureInputChunk::Text(t_mut) = &mut self.chunks[i] {
                                t_mut.remove(prev);
                            }
                            self.cursor = flat + prev;
                            self.merge_adjacent_text();
                            self.snap_cursor();
                            return true;
                        }
                        if off == 0
                            && i > 0
                            && matches!(self.chunks[i - 1], FeatureInputChunk::SkillToken { .. })
                        {
                            self.chunks.remove(i - 1);
                            self.cursor = flat;
                            self.merge_adjacent_text();
                            self.snap_cursor();
                            return true;
                        }
                        return false;
                    }
                    flat = end;
                }
                FeatureInputChunk::SkillToken { name } => {
                    let slen = 1 + name.len();
                    let skill_end = flat + slen;
                    if self.cursor == skill_end {
                        self.chunks.remove(i);
                        self.cursor = flat;
                        self.merge_adjacent_text();
                        self.snap_cursor();
                        return true;
                    }
                    flat += slen;
                }
            }
        }
        false
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let at = self.cursor;
        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            match ch {
                FeatureInputChunk::Text(t) => {
                    let tlen = t.len();
                    let end = flat + tlen;
                    if at > flat && at <= end {
                        let off = at - flat;
                        let prev = prev_char_boundary(t, off);
                        self.cursor = flat + prev;
                        return;
                    }
                    if at == flat && i > 0 {
                        if let FeatureInputChunk::SkillToken { name } = &self.chunks[i - 1] {
                            self.cursor = flat - (1 + name.len());
                            return;
                        }
                    }
                    flat = end;
                }
                FeatureInputChunk::SkillToken { name } => {
                    let slen = 1 + name.len();
                    let skill_start = flat;
                    let skill_end = flat + slen;
                    if at == skill_end {
                        self.cursor = skill_start;
                        return;
                    }
                    if at == skill_start && i > 0 {
                        if let FeatureInputChunk::Text(prev) = &self.chunks[i - 1] {
                            self.cursor = flat.saturating_sub(prev.len());
                        }
                        return;
                    }
                    flat += slen;
                }
            }
        }
        self.cursor = prev_char_boundary(&self.display(), self.cursor);
    }

    pub fn move_right(&mut self) {
        let d = self.display();
        if self.cursor >= d.len() {
            return;
        }
        let at = self.cursor;
        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            match ch {
                FeatureInputChunk::Text(t) => {
                    let tlen = t.len();
                    let end = flat + tlen;
                    if at >= flat && at < end {
                        let off = at - flat;
                        self.cursor = flat + advance_char(t, off);
                        return;
                    }
                    if at == end
                        && i + 1 < self.chunks.len()
                        && matches!(self.chunks[i + 1], FeatureInputChunk::SkillToken { .. })
                    {
                        if let Some(after) = self.flat_after_skill_chunk(i + 1) {
                            self.cursor = after;
                        }
                        return;
                    }
                    flat = end;
                }
                FeatureInputChunk::SkillToken { name } => {
                    let slen = 1 + name.len();
                    let skill_start = flat;
                    let skill_end = flat + slen;
                    if at == skill_start {
                        self.cursor = skill_end;
                        return;
                    }
                    flat += slen;
                }
            }
        }
        self.cursor = advance_char(&d, self.cursor);
    }

    /// Remove the slash command suffix starting at `trigger_flat` (byte index of `/`).
    pub fn truncate_at_slash_trigger(&mut self, trigger_flat: usize) {
        let mut flat = 0usize;
        for (i, ch) in self.chunks.iter().enumerate() {
            if let FeatureInputChunk::Text(t) = ch {
                let tlen = t.len();
                if trigger_flat >= flat && trigger_flat < flat + tlen {
                    let cut = trigger_flat - flat;
                    let prefix = t[..cut].to_string();
                    let mut new_chunks: Vec<FeatureInputChunk> = self.chunks[..i].to_vec();
                    new_chunks.push(FeatureInputChunk::Text(prefix));
                    self.chunks = new_chunks;
                    self.merge_adjacent_text();
                    self.cursor = self.cursor.min(trigger_flat);
                    self.snap_cursor();
                    return;
                }
                flat += tlen;
            } else if let FeatureInputChunk::SkillToken { name } = ch {
                flat += 1 + name.len();
            }
        }
    }

    /// After menu confirm: drop `/query`, append skill token and trailing empty text.
    pub fn accept_skill_token(&mut self, skill_name: &str, trigger_flat: usize) {
        self.truncate_at_slash_trigger(trigger_flat);
        self.chunks.push(FeatureInputChunk::SkillToken {
            name: skill_name.to_string(),
        });
        self.chunks.push(FeatureInputChunk::Text(String::new()));
        self.merge_adjacent_text();
        self.cursor = self.display().len();
        self.snap_cursor();
    }

    /// After `/start-*` menu confirm: drop partial `/…`, insert literal (e.g. `/start-tdd`).
    pub fn accept_slash_menu_literal(&mut self, literal: &str, trigger_flat: usize) {
        self.truncate_at_slash_trigger(trigger_flat);
        self.chunks
            .push(FeatureInputChunk::Text(literal.to_string()));
        self.merge_adjacent_text();
        self.cursor = self.display().len();
        self.snap_cursor();
    }
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx <= 1 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn advance_char(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return idx;
    }
    s[idx..]
        .chars()
        .next()
        .map(|c| idx + c.len_utf8())
        .unwrap_or(idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_plain_text_unchanged_trimmed_tail() {
        let mut b = FeatureInputBuffer::new();
        b.set_plain_text("  hello  ");
        assert_eq!(b.submit_expanded(), "hello");
    }

    #[test]
    fn display_shows_slash_skill_submit_expands() {
        let mut b = FeatureInputBuffer::new();
        b.set_plain_text("do thing ");
        let trigger = b.cursor;
        b.insert_char('/');
        assert_eq!(b.display(), "do thing /");
        b.accept_skill_token("foo", trigger);
        assert_eq!(b.display(), "do thing /foo");
        let sub = b.submit_expanded();
        assert!(sub.contains("[Skill: @.agents/skills/foo"));
        assert!(sub.contains("do thing"));
    }

    #[test]
    fn backspace_after_skill_removes_whole_token() {
        let mut b = FeatureInputBuffer::new();
        b.set_plain_text("x ");
        let trigger = b.display().len();
        b.insert_char('/');
        b.accept_skill_token("bar", trigger);
        assert_eq!(b.display(), "x /bar");
        b.cursor = b.display().len();
        assert!(b.backspace());
        assert_eq!(b.display(), "x ");
    }

    #[test]
    fn prompt_segments_split_text_and_skill() {
        let mut b = FeatureInputBuffer::new();
        b.set_plain_text("x ");
        let trigger = b.cursor;
        b.insert_char('/');
        b.accept_skill_token("foo", trigger);
        let segs = b.prompt_segments();
        assert!(matches!(segs[0], FeaturePromptSegment::Plain(ref s) if s == "x "));
        assert!(matches!(segs[1], FeaturePromptSegment::SkillName(ref n) if n == "foo"));
    }

    #[test]
    fn left_skips_skill_in_one_step_from_after() {
        let mut b = FeatureInputBuffer::new();
        b.set_plain_text("a");
        let trigger = b.display().len();
        b.insert_char('/');
        b.accept_skill_token("s", trigger);
        b.insert_char('b');
        assert_eq!(b.display(), "a/sb");
        b.cursor = b.display().len();
        b.move_left();
        assert_eq!(b.cursor, "a/s".len());
        b.move_left();
        assert_eq!(b.cursor, "a".len());
    }
}
