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
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl Document {
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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

    fn undo(&mut self) {
        let Some(prev) = self.undo_stack.pop() else {
            return;
        };

        self.redo_stack.push(self.snapshot());
        self.restore_from_snapshot(prev);
    }

    fn redo(&mut self) {
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

        if self.row < self.scroll_row {
            self.scroll_row = self.row;
        }

        if self.row >= self.scroll_row + content_h {
            self.scroll_row = self.row + 1 - content_h;
        }

        let w = editor_width.max(1);
        if self.col < self.hscroll {
            self.hscroll = self.col;
        }

        if self.col >= self.hscroll + w {
            self.hscroll = self.col + 1 - w;
        }
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
    pub fn handle_key(&mut self, key: Key) -> io::Result<bool> {
        match key {
            Key::CtrlQ => {
                if self.dirty && !self.force_quit_pending {
                    self.force_quit_pending = true;
                    return Ok(false);
                }

                return Ok(true);
            }
            Key::CtrlS => {
                self.save()?;
            }
            Key::CtrlC => {
                self.undo();
            }
            Key::CtrlV => {
                self.redo();
            }
            Key::Char(ch) => {
                self.push_undo_snapshot();
                let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Enter => {
                self.push_undo_snapshot();
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\n');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Tab => {
                self.push_undo_snapshot();
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\t');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Backspace => {
                self.push_undo_snapshot();
                if let Some((r, c)) = self.buffer.backspace(self.row, self.col) {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::Delete => {
                self.push_undo_snapshot();
                if let Some((r, c)) = self.buffer.delete_forward(self.row, self.col) {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::ArrowLeft => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.buffer.line_len_chars(self.row);
                }
            }
            Key::ArrowRight => {
                let len = self.buffer.line_len_chars(self.row);
                if self.col < len {
                    self.col += 1;
                } else if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            Key::ArrowUp => {
                if self.row > 0 {
                    self.row -= 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::ArrowDown => {
                if self.row + 1 < self.buffer.line_count() {
                    self.row += 1;
                    self.col = self.col.min(self.buffer.line_len_chars(self.row));
                }
            }
            Key::Home => self.col = 0,
            Key::End => self.col = self.buffer.line_len_chars(self.row),
            Key::PageUp => {
                let step = content_height();
                self.row = self.row.saturating_sub(step);
            }
            Key::PageDown => {
                let step = content_height();
                self.row = (self.row + step).min(self.buffer.line_count().saturating_sub(1));
                self.col = self.col.min(self.buffer.line_len_chars(self.row));
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
            | Key::CtrlK
            | Key::CtrlN
            | Key::CtrlO
            | Key::CtrlU
            | Key::CtrlW
            | Key::CtrlP
            | Key::CtrlX
            | Key::CtrlZ
            | Key::CtrlArrowLeft
            | Key::CtrlArrowRight
            | Key::CtrlArrowUp
            | Key::CtrlArrowDown
            | Key::CtrlBackslash => {}
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
}
