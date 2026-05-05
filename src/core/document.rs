use std::fs;
use std::io;
use std::path::PathBuf;

use crate::core::buffer::Buffer;
use crate::core::keys::Key;
use crate::core::terminal::{winsize_tty, TermSize};

#[derive(Clone)]
struct Snapshot {
    text: String,
    row: usize,
    col: usize,
    scroll_row: usize,
    hscroll: usize,
    dirty: bool,
    selection: Option<SelectionRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditKind {
    InsertChar,
    InsertTab,
    Backspace,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

pub struct Document {
    pub buffer: Buffer,
    pub row: usize,
    pub col: usize,
    pub scroll_row: usize,
    pub hscroll: usize,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub pinned: bool,
    pub force_quit_pending: bool,
    selection: Option<SelectionRange>,
    selection_history: Vec<SelectionRange>,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    /// После прокрутки колесом вертикальная подгонка `scroll_row` к курсору отключена, пока пользователь снова не двигает курсор с клавиатуры или не кликнет
    vertical_scroll_detached: bool,
    last_edit_kind: Option<EditKind>,
}

impl Document {
    fn is_pair(open: char, close: char) -> bool {
        matches!(
            (open, close),
            ('(', ')') | ('{', '}') | ('[', ']') | ('"', '"') | ('\'', '\'')
        )
    }

    fn line_char_at(&self, row: usize, col: usize) -> Option<char> {
        self.buffer.lines().get(row).and_then(|line| line.chars().nth(col))
    }

    fn smart_backspace_indent(&mut self, tab_size: usize, insert_spaces: bool) -> Option<(usize, usize)> {
        if !insert_spaces || self.col == 0 {
            return None;
        }

        let line = self.buffer.lines().get(self.row)?;
        if !line.chars().take(self.col).all(|ch| ch == ' ') {
            return None;
        }

        let step = tab_size.max(1);
        let to_delete = self.col % step;
        let delete_count = if to_delete == 0 { step } else { to_delete };
        let mut pos = (self.row, self.col);
        for _ in 0..delete_count {
            if let Some(next) = self.buffer.backspace(pos.0, pos.1) {
                pos = next;
            } else {
                break;
            }
        }
        Some(pos)
    }

    fn smart_backspace_pair(&mut self) -> Option<(usize, usize)> {
        if self.col == 0 {
            return None;
        }
        let prev = self.line_char_at(self.row, self.col.saturating_sub(1))?;
        let next = self.line_char_at(self.row, self.col)?;
        if !Self::is_pair(prev, next) {
            return None;
        }

        let (r, c) = self.buffer.backspace(self.row, self.col)?;
        let (r, c) = self.buffer.delete_forward(r, c)?;
        Some((r, c))
    }

    fn smart_delete_pair(&mut self) -> Option<(usize, usize)> {
        let cur = self.line_char_at(self.row, self.col)?;
        let next = self.line_char_at(self.row, self.col + 1)?;
        if !Self::is_pair(cur, next) {
            return None;
        }

        let (r, c) = self.buffer.delete_forward(self.row, self.col)?;
        let (r, c) = self.buffer.delete_forward(r, c)?;
        Some((r, c))
    }

    fn duplicate_current_line(&mut self) -> bool {
        let mut lines = self.buffer.lines().to_vec();
        if self.row >= lines.len() {
            return false;
        }

        let line = lines[self.row].clone();
        lines.insert(self.row + 1, line);
        self.buffer = Buffer::from_file(&lines.join("\n"));
        self.row = (self.row + 1).min(self.buffer.line_count().saturating_sub(1));
        self.col = self.col.min(self.buffer.line_len_chars(self.row));
        true
    }

    fn move_current_line_up(&mut self) -> bool {
        if self.row == 0 {
            return false;
        }

        let mut lines = self.buffer.lines().to_vec();
        if self.row >= lines.len() {
            return false;
        }

        lines.swap(self.row - 1, self.row);
        self.buffer = Buffer::from_file(&lines.join("\n"));
        self.row -= 1;
        self.col = self.col.min(self.buffer.line_len_chars(self.row));
        true
    }

    fn move_current_line_down(&mut self) -> bool {
        let mut lines = self.buffer.lines().to_vec();
        if self.row + 1 >= lines.len() {
            return false;
        }

        lines.swap(self.row, self.row + 1);
        self.buffer = Buffer::from_file(&lines.join("\n"));
        self.row += 1;
        self.col = self.col.min(self.buffer.line_len_chars(self.row));
        true
    }

    fn auto_pair_for(ch: char) -> Option<char> {
        match ch {
            '(' => Some(')'),
            '{' => Some('}'),
            '[' => Some(']'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        }
    }

    fn insert_char_with_auto_pair(&mut self, ch: char) {
        let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
        self.row = r;
        self.col = c;
        if let Some(closing) = Self::auto_pair_for(ch) {
            let original_row = self.row;
            let original_col = self.col;
            let (_r2, _c2) = self.buffer.insert_char(self.row, self.col, closing);
            self.row = original_row;
            self.col = original_col;
        }
        self.dirty = true;
    }

    fn insert_tab(&mut self, tab_size: usize, insert_spaces: bool) {
        if insert_spaces {
            let spaces = " ".repeat(tab_size.max(1));
            for ch in spaces.chars() {
                let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
                self.row = r;
                self.col = c;
            }
        } else {
            let (r, c) = self.buffer.insert_char(self.row, self.col, '\t');
            self.row = r;
            self.col = c;
        }
        self.dirty = true;
    }

    pub fn empty() -> Self {
        Self {
            buffer: Buffer::new(),
            row: 0,
            col: 0,
            scroll_row: 0,
            hscroll: 0,
            path: None,
            dirty: false,
            pinned: false,
            force_quit_pending: false,
            selection: None,
            selection_history: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            vertical_scroll_detached: false,
            last_edit_kind: None,
        }
    }

    pub fn open_file(path: PathBuf) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?;
        Ok(Self {
            buffer: Buffer::from_file(&text),
            row: 0,
            col: 0,
            scroll_row: 0,
            hscroll: 0,
            path: Some(path),
            dirty: false,
            pinned: false,
            force_quit_pending: false,
            selection: None,
            selection_history: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            vertical_scroll_detached: false,
            last_edit_kind: None,
        })
    }

    pub fn new_file(path: PathBuf) -> Self {
        Self {
            buffer: Buffer::new(),
            row: 0,
            col: 0,
            scroll_row: 0,
            hscroll: 0,
            path: Some(path),
            dirty: false,
            pinned: false,
            force_quit_pending: false,
            selection: None,
            selection_history: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            vertical_scroll_detached: false,
            last_edit_kind: None,
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.buffer.to_file_string(),
            row: self.row,
            col: self.col,
            scroll_row: self.scroll_row,
            hscroll: self.hscroll,
            dirty: self.dirty,
            selection: self.selection.clone(),
        }
    }

    fn restore_from_snapshot(&mut self, snap: Snapshot) {
        self.buffer = Buffer::from_file(&snap.text);
        self.row = snap.row;
        self.col = snap.col;
        self.scroll_row = snap.scroll_row;
        self.hscroll = snap.hscroll;
        self.dirty = snap.dirty;
        self.selection = snap.selection;
        self.vertical_scroll_detached = false;
        self.last_edit_kind = None;
        self.clamp_cursor();
    }

    fn push_undo_snapshot(&mut self) {
        let snap = self.snapshot();
        if let Some(last) = self.undo_stack.last() {
            if last.text == snap.text && last.row == snap.row && last.col == snap.col {
                return;
            }
        }

        self.undo_stack.push(snap);
        if self.undo_stack.len() > 256 {
            let _ = self.undo_stack.remove(0);
        }

        self.redo_stack.clear();
    }

    fn begin_grouped_edit(&mut self, kind: EditKind) {
        if self.last_edit_kind != Some(kind) {
            self.push_undo_snapshot();
        }
        self.last_edit_kind = Some(kind);
    }

    fn break_edit_group(&mut self) {
        self.last_edit_kind = None;
    }

    fn undo(&mut self) {
        self.last_edit_kind = None;
        let Some(prev) = self.undo_stack.pop() else {
            return;
        };

        self.redo_stack.push(self.snapshot());
        self.restore_from_snapshot(prev);
    }

    fn redo(&mut self) {
        self.last_edit_kind = None;
        let Some(next) = self.redo_stack.pop() else {
            return;
        };

        self.undo_stack.push(self.snapshot());
        self.restore_from_snapshot(next);
    }

    pub fn save(&mut self) -> io::Result<()> {
        if let Some(ref p) = self.path {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(p, self.buffer.to_file_string())?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn clamp_cursor(&mut self) {
        let n = self.buffer.line_count();
        if self.row >= n {
            self.row = n.saturating_sub(1);
        }

        let len = self.buffer.line_len_chars(self.row);
        if self.col > len {
            self.col = len;
        }
    }

    pub fn adjust_scroll(&mut self, content_h: usize, editor_width: usize) {
        if content_h == 0 {
            return;
        }

        if !self.vertical_scroll_detached {
            if self.row < self.scroll_row {
                self.scroll_row = self.row;
            }

            if self.row >= self.scroll_row + content_h {
                self.scroll_row = self.row + 1 - content_h;
            }
        }

        let w = editor_width.max(1);
        if self.col < self.hscroll {
            self.hscroll = self.col;
        }

        if self.col >= self.hscroll + w {
            self.hscroll = self.col + 1 - w;
        }
    }

    /// Прокрутка только вьюпорта; `row`/`col` не меняются. Шаг задаётся событием колеса
    pub fn scroll_viewport_lines(&mut self, delta: isize, viewport_h: usize) {
        let n = self.buffer.line_count().max(1);
        let vh = viewport_h.max(1);
        let max_scroll = if n > vh { n - vh } else { 0 };
        let cur = self.scroll_row as isize + delta;
        self.scroll_row = cur.clamp(0, max_scroll as isize) as usize;
        self.vertical_scroll_detached = true;
    }

    /// Выключает режим «колесо сдвинуло viewport без привязки к курсору» (клавиатура, клик)
    pub fn clear_vertical_scroll_detachment(&mut self) {
        self.vertical_scroll_detached = false;
    }

    /// Одна строка текста для окна редактора (ширина `max_chars`)
    pub fn editor_line_display(&self, line_idx: usize, max_chars: usize) -> String {
        self.buffer
            .lines()
            .get(line_idx)
            .map(|s| {
                let skip = self.hscroll.min(s.chars().count());
                s.chars().skip(skip).take(max_chars).collect()
            })
            .unwrap_or_default()
    }

    pub fn path_display(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.to_str())
            .map(String::from)
            .unwrap_or_else(|| "[new]".to_string())
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.break_edit_group();

        if self.replace_selection_with_text(text) {
            return;
        }

        self.push_undo_snapshot();
        for ch in text.chars() {
            let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
            self.row = r;
            self.col = c;
        }

        self.dirty = true;
        self.clamp_cursor();
    }

    pub fn set_selection(
        &mut self,
        start_row: usize,
        start_col: usize,
        end_row: usize,
        end_col: usize,
    ) {
        self.selection = Some(normalize_selection(
            start_row, start_col, end_row, end_col,
        ));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_history.clear();
    }

    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    fn select_word_at_cursor(&mut self) -> bool {
        let Some(line) = self.buffer.lines().get(self.row) else {
            return false;
        };
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return false;
        }

        let len = chars.len();
        let mut pos = self.col.min(len.saturating_sub(1));
        if !Self::is_word_char(chars[pos]) && pos > 0 && Self::is_word_char(chars[pos - 1]) {
            pos -= 1;
        }
        if !Self::is_word_char(chars[pos]) {
            return false;
        }

        let mut start = pos;
        while start > 0 && Self::is_word_char(chars[start - 1]) {
            start -= 1;
        }
        let mut end = pos + 1;
        while end < len && Self::is_word_char(chars[end]) {
            end += 1;
        }

        self.selection = Some(SelectionRange {
            start_row: self.row,
            start_col: start,
            end_row: self.row,
            end_col: end,
        });
        true
    }

    fn move_word_left(&mut self) {
        if self.col == 0 {
            if self.row == 0 {
                return;
            }

            self.row -= 1;
            self.col = self.buffer.line_len_chars(self.row);
        }

        let Some(line) = self.buffer.lines().get(self.row) else {
            return;
        };

        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            self.col = 0;
            return;
        }

        let mut i = self.col.min(chars.len());
        while i > 0 && !Self::is_word_char(chars[i - 1]) {
            i -= 1;
        }

        while i > 0 && Self::is_word_char(chars[i - 1]) {
            i -= 1;
        }

        self.col = i;
    }

    fn move_word_right(&mut self) {
        let len = self.buffer.line_len_chars(self.row);
        if self.col >= len {
            if self.row + 1 < self.buffer.line_count() {
                self.row += 1;
                self.col = 0;
            }
            return;
        }

        let Some(line) = self.buffer.lines().get(self.row) else {
            return;
        };
        let chars: Vec<char> = line.chars().collect();
        let mut i = self.col.min(chars.len());

        if i < chars.len() && Self::is_word_char(chars[i]) {
            while i < chars.len() && Self::is_word_char(chars[i]) {
                i += 1;
            }
        }

        while i < chars.len() && !Self::is_word_char(chars[i]) {
            i += 1;
        }

        self.col = i;
    }

    pub fn expand_selection(&mut self) -> bool {
        let current = self.selection.clone();
        match current {
            None => self.select_word_at_cursor(),
            Some(sel) => {
                self.selection_history.push(sel.clone());
                let line_len = self.buffer.line_len_chars(sel.start_row);
                if sel.start_row == sel.end_row && !(sel.start_col == 0 && sel.end_col == line_len) {
                    self.selection = Some(SelectionRange {
                        start_row: sel.start_row,
                        start_col: 0,
                        end_row: sel.end_row,
                        end_col: line_len,
                    });
                    true
                } else {
                    let last_row = self.buffer.line_count().saturating_sub(1);
                    let last_col = self.buffer.line_len_chars(last_row);
                    if sel.start_row == 0 && sel.start_col == 0 && sel.end_row == last_row && sel.end_col == last_col {
                        false
                    } else {
                        self.selection = Some(SelectionRange {
                            start_row: 0,
                            start_col: 0,
                            end_row: last_row,
                            end_col: last_col,
                        });
                        true
                    }
                }
            }
        }
    }

    pub fn shrink_selection(&mut self) -> bool {
        if let Some(prev) = self.selection_history.pop() {
            self.selection = Some(prev);
            true
        } else if self.selection.is_some() {
            self.selection = None;
            true
        } else {
            false
        }
    }

    fn replace_selection_with_text(&mut self, text: &str) -> bool {
        let Some(sel) = self.selection.clone() else {
            return false;
        };

        // Работаем через линейное представление, чтобы корректно заменить диапазон в том числе на границах строк.
        let content = self.buffer.to_file_string();
        let Some(start_off) = position_to_offset(&content, sel.start_row, sel.start_col) else {
            self.selection = None;
            return false;
        };
        let Some(end_off) = position_to_offset(&content, sel.end_row, sel.end_col) else {
            self.selection = None;
            return false;
        };
        if start_off > end_off {
            self.selection = None;
            return false;
        }

        // Замена выделения должна быть одной undo-операцией
        self.push_undo_snapshot();
        let mut next = String::with_capacity(content.len() + text.len());
        next.push_str(&content[..start_off]);
        next.push_str(text);
        next.push_str(&content[end_off..]);

        self.buffer = Buffer::from_file(&next);

        // Возвращаем курсор в конец вставленного текста
        let (row, col) = offset_to_position(&next, start_off + text.chars().count());
        self.row = row;
        self.col = col;
        self.clear_selection();
        self.dirty = true;
        self.clamp_cursor();
        true
    }

    /// `true` = завершить приложение
    pub fn handle_key_with_config(&mut self, key: Key, tab_size: usize, insert_spaces: bool) -> io::Result<bool> {
        self.vertical_scroll_detached = false;
        match key {
            Key::CtrlQ => {
                if self.dirty && !self.force_quit_pending {
                    self.force_quit_pending = true;
                    return Ok(false);
                }

                return Ok(true);
            }
            Key::CtrlS => {
                self.break_edit_group();
                self.save()?;
            }
            Key::CtrlC => {
                self.break_edit_group();
                self.undo();
            }
            Key::CtrlV => {
                self.break_edit_group();
                self.redo();
            }
            Key::CtrlK => {
                self.break_edit_group();
                self.push_undo_snapshot();
                if self.duplicate_current_line() {
                    self.dirty = true;
                }
            }
            Key::CtrlO => {
                self.break_edit_group();
                self.push_undo_snapshot();
                if self.move_current_line_up() {
                    self.dirty = true;
                }
            }
            Key::CtrlN => {
                self.break_edit_group();
                self.push_undo_snapshot();
                if self.move_current_line_down() {
                    self.dirty = true;
                }
            }
            Key::Char(ch) => {
                self.begin_grouped_edit(EditKind::InsertChar);
                self.insert_char_with_auto_pair(ch);
            }
            Key::Enter => {
                self.break_edit_group();
                self.push_undo_snapshot();
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\n');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Tab => {
                self.begin_grouped_edit(EditKind::InsertTab);
                self.insert_tab(tab_size, insert_spaces);
            }
            Key::Backspace => {
                self.begin_grouped_edit(EditKind::Backspace);
                if let Some((r, c)) = self
                    .smart_backspace_pair()
                    .or_else(|| self.smart_backspace_indent(tab_size, insert_spaces))
                    .or_else(|| self.buffer.backspace(self.row, self.col))
                {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::Delete => {
                self.begin_grouped_edit(EditKind::Delete);
                if let Some((r, c)) = self
                    .smart_delete_pair()
                    .or_else(|| self.buffer.delete_forward(self.row, self.col))
                {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::ArrowLeft => {
                self.break_edit_group();
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.buffer.line_len_chars(self.row);
                }
            }
            Key::ArrowRight => {
                self.break_edit_group();
                let len = self.buffer.line_len_chars(self.row);
                if self.col < len {
                    self.col += 1;
                } else if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            Key::ArrowUp => {
                self.break_edit_group();
                if self.row > 0 {
                    self.row -= 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::ArrowDown => {
                self.break_edit_group();
                if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::Home => {
                self.break_edit_group();
                self.col = 0;
            }
            Key::End => {
                self.break_edit_group();
                self.col = self.buffer.line_len_chars(self.row);
            }
            Key::PageUp => {
                self.break_edit_group();
                let step = content_height();
                self.row = self.row.saturating_sub(step);
            }
            Key::PageDown => {
                self.break_edit_group();
                let step = content_height();
                self.row = (self.row + step).min(self.buffer.line_count().saturating_sub(1));
                self.col = self.col.min(self.buffer.line_len_chars(self.row));
            }
            Key::CtrlArrowUp => {
                self.break_edit_group();
                let _ = self.expand_selection();
            }
            Key::CtrlArrowDown => {
                self.break_edit_group();
                let _ = self.shrink_selection();
            }
            Key::CtrlArrowLeft => {
                self.break_edit_group();
                self.move_word_left();
            }
            Key::CtrlArrowRight => {
                self.break_edit_group();
                self.move_word_right();
            }
            Key::Esc
            | Key::ShiftTab
            | Key::CtrlB
            | Key::CtrlA
            | Key::CtrlT
            | Key::CtrlY
            | Key::CtrlD
            | Key::CtrlE
            | Key::CtrlF
            | Key::CtrlG
            | Key::CtrlR
            | Key::CtrlL
            | Key::CtrlJ
            | Key::CtrlH
            | Key::CtrlU
            | Key::CtrlW
            | Key::CtrlP
            | Key::CtrlX
            | Key::CtrlZ
            | Key::CtrlBackslash => {
                self.break_edit_group();
            }
        }
        self.force_quit_pending = false;
        Ok(false)
    }

}

fn normalize_selection(
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
) -> SelectionRange {
    if (start_row, start_col) <= (end_row, end_col) {
        SelectionRange {
            start_row,
            start_col,
            end_row,
            end_col,
        }
    } else {
        SelectionRange {
            start_row: end_row,
            start_col: end_col,
            end_row: start_row,
            end_col: start_col,
        }
    }
}

fn position_to_offset(text: &str, row: usize, col: usize) -> Option<usize> {
    let mut cur_row = 0usize;
    let mut cur_col = 0usize;
    for (byte_idx, ch) in text.char_indices() {
        if cur_row == row && cur_col == col {
            return Some(byte_idx);
        }
        if ch == '\n' {
            cur_row += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
    }

    if cur_row == row && cur_col == col {
        return Some(text.len());
    }

    None
}

fn offset_to_position(text: &str, target_chars: usize) -> (usize, usize) {
    let mut row = 0usize;
    let mut col = 0usize;
    for (count, ch) in text.chars().enumerate() {
        if count == target_chars {
            return (row, col);
        }
        
        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

fn content_height() -> usize {
    let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
    (size.rows.saturating_sub(1)).max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::Document;
    use crate::core::buffer::Buffer;
    use crate::core::keys::Key;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn insert_text_replaces_selection() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("hello world");
        doc.set_selection(0, 6, 0, 11);
        doc.insert_text("tce");
        assert_eq!(doc.buffer.to_file_string(), "hello tce");
    }

    #[test]
    fn new_file_keeps_target_path() {
        let path = PathBuf::from("/tmp/new-file.txt");
        let doc = Document::new_file(path.clone());
        assert_eq!(doc.path, Some(path));
        assert!(!doc.dirty);
        assert_eq!(doc.buffer.to_file_string(), "");
    }

    #[test]
    fn save_creates_missing_parent_dirs() {
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tce-save-test-{uniq}"));
        let path = root.join("nested/dir/file.txt");

        let mut doc = Document::new_file(path.clone());
        doc.insert_text("hello");
        doc.save().expect("save should create parent directories");

        let saved = fs::read_to_string(&path).expect("saved file should exist");
        assert_eq!(saved, "hello");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opening_bracket_inserts_pair_and_keeps_cursor_inside() {
        let mut doc = Document::empty();
        doc.handle_key_with_config(Key::Char('('), 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "()");
        assert_eq!(doc.row, 0);
        assert_eq!(doc.col, 1);
    }

    #[test]
    fn quote_inserts_pair_and_keeps_cursor_inside() {
        let mut doc = Document::empty();
        doc.handle_key_with_config(Key::Char('"'), 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "\"\"");
        assert_eq!(doc.row, 0);
        assert_eq!(doc.col, 1);
    }

    #[test]
    fn backspace_between_pair_removes_both_chars() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("()");
        doc.row = 0;
        doc.col = 1;

        doc.handle_key_with_config(Key::Backspace, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "");
        assert_eq!(doc.col, 0);
    }

    #[test]
    fn delete_on_opening_pair_removes_both_chars() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("[]");
        doc.row = 0;
        doc.col = 0;

        doc.handle_key_with_config(Key::Delete, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "");
        assert_eq!(doc.col, 0);
    }

    #[test]
    fn backspace_in_indentation_deletes_to_tab_stop() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("        x");
        doc.row = 0;
        doc.col = 8;

        doc.handle_key_with_config(Key::Backspace, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "    x");
        assert_eq!(doc.col, 4);
    }

    #[test]
    fn ctrl_k_duplicates_current_line() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("a\nb");
        doc.row = 0;
        doc.col = 1;

        doc.handle_key_with_config(Key::CtrlK, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "a\na\nb");
        assert_eq!(doc.row, 1);
    }

    #[test]
    fn ctrl_o_moves_line_up() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("one\ntwo\nthree");
        doc.row = 1;
        doc.col = 2;

        doc.handle_key_with_config(Key::CtrlO, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "two\none\nthree");
        assert_eq!(doc.row, 0);
    }

    #[test]
    fn ctrl_n_moves_line_down() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("one\ntwo\nthree");
        doc.row = 1;
        doc.col = 1;

        doc.handle_key_with_config(Key::CtrlN, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "one\nthree\ntwo");
        assert_eq!(doc.row, 2);
    }

    #[test]
    fn sequential_typing_is_grouped_into_single_undo_step() {
        let mut doc = Document::empty();
        doc.handle_key_with_config(Key::Char('a'), 4, true).expect("key handling should work");
        doc.handle_key_with_config(Key::Char('b'), 4, true).expect("key handling should work");
        doc.handle_key_with_config(Key::Char('c'), 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "abc");

        doc.handle_key_with_config(Key::CtrlC, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "");
    }

    #[test]
    fn cursor_move_breaks_edit_group_for_undo() {
        let mut doc = Document::empty();
        doc.handle_key_with_config(Key::Char('a'), 4, true).expect("key handling should work");
        doc.handle_key_with_config(Key::Char('b'), 4, true).expect("key handling should work");
        doc.handle_key_with_config(Key::ArrowLeft, 4, true).expect("key handling should work");
        doc.handle_key_with_config(Key::Char('x'), 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "axb");

        doc.handle_key_with_config(Key::CtrlC, 4, true).expect("key handling should work");
        assert_eq!(doc.buffer.to_file_string(), "ab");
    }

    #[test]
    fn expand_and_shrink_selection_cycle() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("alpha beta");
        doc.row = 0;
        doc.col = 1;

        doc.handle_key_with_config(Key::CtrlArrowUp, 4, true).expect("key handling should work");
        let sel1 = doc.selection.clone().expect("word selection");
        assert_eq!((sel1.start_col, sel1.end_col), (0, 5));

        doc.handle_key_with_config(Key::CtrlArrowUp, 4, true).expect("key handling should work");
        let sel2 = doc.selection.clone().expect("line selection");
        assert_eq!((sel2.start_col, sel2.end_col), (0, 10));

        doc.handle_key_with_config(Key::CtrlArrowDown, 4, true).expect("key handling should work");
        let sel3 = doc.selection.clone().expect("shrunk selection");
        assert_eq!((sel3.start_col, sel3.end_col), (0, 5));

        doc.handle_key_with_config(Key::CtrlArrowDown, 4, true).expect("key handling should work");
        assert!(doc.selection.is_none());
    }

    #[test]
    fn ctrl_word_jump_supports_utf8_words() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("привет мир");
        doc.row = 0;
        doc.col = 0;

        doc.handle_key_with_config(Key::CtrlArrowRight, 4, true).expect("key handling should work");
        assert_eq!(doc.col, 7);

        doc.handle_key_with_config(Key::CtrlArrowLeft, 4, true).expect("key handling should work");
        assert_eq!(doc.col, 0);
    }
}
