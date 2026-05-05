use crate::core::document::Document;

pub fn apply_quality_highlights(
    rendered: &str,
    visible_raw: &str,
    full_line: &str,
    line_idx: usize,
    hscroll: usize,
    bracket_pair: &Option<((usize, usize), (usize, usize))>,
    scope_range: Option<(usize, usize)>,
    is_current_line: bool,
) -> String {
    let visible_chars: Vec<char> = visible_raw.chars().collect();
    if visible_chars.is_empty() {
        return rendered.to_string();
    }

    let mut bracket_cols = Vec::<usize>::new();
    if let Some((a, b)) = bracket_pair {
        if a.0 == line_idx {
            bracket_cols.push(a.1);
        }

        if b.0 == line_idx {
            bracket_cols.push(b.1);
        }
    }

    let mixed_indent = has_mixed_indent(full_line);
    let indent_len = full_line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    let trailing_len = full_line
        .chars()
        .rev()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    let full_len = full_line.chars().count();
    let trailing_start = full_len.saturating_sub(trailing_len);
    let in_scope = scope_range
        .map(|(start, end)| line_idx >= start && line_idx <= end)
        .unwrap_or(false);
    if bracket_cols.is_empty() && !mixed_indent && trailing_len == 0 && !in_scope {
        return rendered.to_string();
    }

    let mut out = String::new();
    if in_scope && !is_current_line {
        out.push_str("\x1b[48;5;236m");
    }

    for (local_idx, ch) in visible_chars.iter().enumerate() {
        let abs_col = hscroll + local_idx;
        if bracket_cols.contains(&abs_col) {
            out.push_str("\x1b[1;96m");
            out.push(*ch);
            out.push_str("\x1b[0m");
            if in_scope && !is_current_line {
                out.push_str("\x1b[48;5;236m");
            }

            continue;
        }

        if mixed_indent && abs_col < indent_len && (*ch == ' ' || *ch == '\t') {
            out.push_str("\x1b[48;5;52m");
            out.push(*ch);
            out.push_str("\x1b[0m");
            if in_scope && !is_current_line {
                out.push_str("\x1b[48;5;236m");
            }

            continue;
        }

        if abs_col >= trailing_start && (*ch == ' ' || *ch == '\t') {
            out.push_str("\x1b[48;5;52m");
            out.push(*ch);
            out.push_str("\x1b[0m");
            if in_scope && !is_current_line {
                out.push_str("\x1b[48;5;236m");
            }

            continue;
        }

        out.push(*ch);
    }

    if in_scope && !is_current_line {
        out.push_str("\x1b[0m");
    }

    out
}

pub fn has_mixed_indent(line: &str) -> bool {
    let indent: Vec<char> = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    indent.contains(&' ') && indent.contains(&'\t')
}

fn is_bracket_char(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}')
}

fn bracket_match(open: char, close: char) -> bool {
    matches!((open, close), ('(', ')') | ('[', ']') | ('{', '}'))
}

fn flatten_doc_chars(doc: &Document) -> Vec<(usize, usize, char)> {
    let mut out = Vec::new();
    for (row, line) in doc.buffer.lines().iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            out.push((row, col, ch));
        }
    }

    out
}

pub fn find_matching_bracket_pair(doc: &Document) -> Option<((usize, usize), (usize, usize))> {
    let chars = flatten_doc_chars(doc);
    if chars.is_empty() {
        return None;
    }

    let candidate = chars
        .iter()
        .enumerate()
        .find(|(_, (r, c, ch))| *r == doc.row && *c == doc.col && is_bracket_char(*ch))
        .or_else(|| {
            doc.col.checked_sub(1).and_then(|left| {
                chars
                    .iter()
                    .enumerate()
                    .find(|(_, (r, c, ch))| *r == doc.row && *c == left && is_bracket_char(*ch))
            })
        })?;

    let (idx, (row, col, ch)) = candidate;
    if matches!(*ch, '(' | '[' | '{') {
        let mut depth = 0usize;
        for (_j, (r, c, cur)) in chars.iter().enumerate().skip(idx + 1) {
            if *cur == *ch {
                depth += 1;
            } else if bracket_match(*ch, *cur) {
                if depth == 0 {
                    return Some(((*row, *col), (*r, *c)));
                }
                depth = depth.saturating_sub(1);
            }
        }
    } else {
        let open = match *ch {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            _ => return None,
        };
        let mut depth = 0usize;
        for (_j, (r, c, cur)) in chars.iter().enumerate().take(idx).rev() {
            if *cur == *ch {
                depth += 1;
            } else if *cur == open {
                if depth == 0 {
                    return Some(((*r, *c), (*row, *col)));
                }
                depth = depth.saturating_sub(1);
            }
        }
    }
    None
}

pub fn find_brace_scope_range(doc: &Document) -> Option<(usize, usize)> {
    let chars = flatten_doc_chars(doc);
    if chars.is_empty() {
        return None;
    }

    let mut cursor_idx = chars
        .iter()
        .position(|(r, c, _)| *r == doc.row && *c >= doc.col)
        .unwrap_or(chars.len().saturating_sub(1));
    if cursor_idx >= chars.len() {
        cursor_idx = chars.len().saturating_sub(1);
    }

    let mut depth = 0usize;
    let mut open_idx = None;
    for i in (0..=cursor_idx).rev() {
        let ch = chars[i].2;
        if ch == '}' {
            depth += 1;
        } else if ch == '{' {
            if depth == 0 {
                open_idx = Some(i);
                break;
            }
            depth = depth.saturating_sub(1);
        }
    }

    let open = open_idx?;
    depth = 0;
    for i in open + 1..chars.len() {
        let ch = chars[i].2;
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            if depth == 0 {
                return Some((chars[open].0, chars[i].0));
            }

            depth = depth.saturating_sub(1);
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::{find_brace_scope_range, find_matching_bracket_pair};
    use crate::core::buffer::Buffer;
    use crate::core::document::Document;

    #[test]
    fn bracket_pair_is_found() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("fn main() { call(a); }");
        doc.row = 0;
        doc.col = 16; // '(' before a
        let pair = find_matching_bracket_pair(&doc).expect("pair exists");
        assert_eq!(pair.0 .0, 0);
        assert_eq!(pair.1 .0, 0);
    }

    #[test]
    fn brace_scope_range_is_found() {
        let mut doc = Document::empty();
        doc.buffer = Buffer::from_file("fn main() {\n  let x = 1;\n}\n");
        doc.row = 1;
        doc.col = 2;
        let scope = find_brace_scope_range(&doc).expect("scope exists");
        assert_eq!(scope, (0, 2));
    }
}
