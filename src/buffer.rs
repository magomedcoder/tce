//! Текстовый буфер: одна `String` на строку
//! Колонка курсора хранится как индекс **символа** (Unicode scalar values)

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    lines: Vec<String>,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
        }
    }

    pub fn from_file(text: &str) -> Self {
        if text.is_empty() {
            return Self::new();
        }

        let mut lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
        for line in &mut lines {
            if line.ends_with('\n') {
                line.pop();
            }

            if line.ends_with('\r') {
                line.pop();
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        Self { lines }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_len_chars(&self, row: usize) -> usize {
        self.lines.get(row).map(|s| s.chars().count()).unwrap_or(0)
    }

    /// Объединяет строки через `\n` для сохранения
    pub fn to_file_string(&self) -> String {
        self.lines.join("\n")
    }

    pub fn insert_char(&mut self, row: usize, col: usize, ch: char) -> (usize, usize) {
        if ch == '\n' || ch == '\r' {
            return self.split_line(row, col);
        }
        
        let line = &mut self.lines[row];
        let bi = byte_index(line, col);
        line.insert(bi, ch);
        (row, col + 1)
    }

    fn split_line(&mut self, row: usize, col: usize) -> (usize, usize) {
        let (bi, tail) = {
            let line = &self.lines[row];
            (byte_index(line, col), line.chars().skip(col).collect::<String>())
        };
        self.lines[row].truncate(bi);
        self.lines.insert(row + 1, tail);
        (row + 1, 0)
    }

    pub fn backspace(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        if col > 0 {
            let line = &mut self.lines[row];
            let bi = byte_index(line, col);
            let prev = prev_char_boundary(line, bi);
            line.remove(prev);
            Some((row, col - 1))
        } else if row > 0 {
            let cur = self.lines.remove(row);
            let prev_len = self.lines[row - 1].chars().count();
            self.lines[row - 1].push_str(&cur);
            Some((row - 1, prev_len))
        } else {
            None
        }
    }

    pub fn delete_forward(&mut self, row: usize, col: usize) -> Option<(usize, usize)> {
        let len = self.line_len_chars(row);
        if col < len {
            let line = &mut self.lines[row];
            let bi = byte_index(line, col);
            line.remove(bi);
            Some((row, col))
        } else if row + 1 < self.lines.len() {
            let next = self.lines.remove(row + 1);
            self.lines[row].push_str(&next);
            Some((row, col))
        } else {
            None
        }
    }
}

fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn prev_char_boundary(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}
