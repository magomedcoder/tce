use std::fs;
use std::io;
use std::path::PathBuf;

use crate::buffer::Buffer;
use crate::keys::Key;
use crate::terminal::{winsize_tty, TermSize};

pub struct Document {
    pub buffer: Buffer,
    pub row: usize,
    pub col: usize,
    pub scroll_row: usize,
    pub hscroll: usize,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub force_quit_pending: bool,
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
            force_quit_pending: false,
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
            force_quit_pending: false,
        })
    }

    pub fn load_file(&mut self, path: PathBuf) -> io::Result<()> {
        let text = fs::read_to_string(&path)?;
        self.buffer = Buffer::from_file(&text);
        self.path = Some(path);
        self.row = 0;
        self.col = 0;
        self.scroll_row = 0;
        self.hscroll = 0;
        self.dirty = false;
        self.force_quit_pending = false;
        Ok(())
    }

    pub fn save(&mut self) -> io::Result<()> {
        if let Some(ref p) = self.path {
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

    /// One line of text for the editor viewport (`max_chars` wide)
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

    /// `true` = quit application.
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
            Key::Char(ch) => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, ch);
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Enter => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\n');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Tab => {
                let (r, c) = self.buffer.insert_char(self.row, self.col, '\t');
                self.row = r;
                self.col = c;
                self.dirty = true;
            }
            Key::Backspace => {
                if let Some((r, c)) = self.buffer.backspace(self.row, self.col) {
                    self.row = r;
                    self.col = c;
                    self.dirty = true;
                }
            }
            Key::Delete => {
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
            Key::Esc | Key::ShiftTab | Key::CtrlB | Key::CtrlL | Key::CtrlK | Key::CtrlN => {}
        }
        self.force_quit_pending = false;
        Ok(false)
    }
}

fn content_height() -> usize {
    let size = winsize_tty().unwrap_or(TermSize { rows: 24, cols: 80 });
    (size.rows.saturating_sub(1)).max(1) as usize
}
