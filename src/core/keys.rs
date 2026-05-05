use std::io;

use crate::core::terminal::read_timeout;

/// События ввода: клавиши и мышь в режиме SGR (CSI `\x1b[<...M|m`)
#[derive(Clone, Copy, Debug)]
pub enum UiEvent {
    Key(Key),
    Mouse(MouseEvent),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// Тип события указателя (xterm SGR 1006)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseEventKind {
    WheelUp,
    WheelDown,
    LeftPress,
    LeftDrag,
    MiddlePress,
    RightPress,
    Release,
    /// Перемещение/нескролловые коды - дальше по цепочке не обрабатываем
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    /// Колонка терминала (1-based, как в CSI)
    pub column: u32,
    /// Строка терминала (1-based, как в CSI)
    pub row: u32,
    pub modifiers: MouseModifiers,
}

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    ShiftTab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    CtrlS,
    CtrlA,
    CtrlT,
    CtrlY,
    CtrlD,
    CtrlE,
    CtrlC,
    CtrlV,
    CtrlF,
    CtrlG,
    CtrlR,
    CtrlQ,
    CtrlB,
    CtrlL,
    CtrlJ,
    CtrlH,
    CtrlK,
    CtrlN,
    CtrlO,
    CtrlU,
    CtrlW,
    CtrlP,
    CtrlX,
    CtrlZ,
    /// Ctrl+\ (0x1c) - поиск в текущем буфере (поиск по проекту остаётся на Ctrl+F)
    CtrlBackslash,
    /// Стиль CSI `1;5D` / SS3 - терминал отправляет его для Ctrl+Left (xterm)
    CtrlArrowLeft,
    CtrlArrowRight,
    CtrlArrowUp,
    CtrlArrowDown,
    Esc,
}

/// Читает одно UI-событие (клавиша или SGR-мышь)
pub fn read_ui_event(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<UiEvent>> {
    let byte = match read_one_byte(stdin_fd)? {
        Some(b) => b,
        None => return Ok(None),
    };

    if byte == 0x1b {
        return parse_escape_event(stdin_fd);
    }

    decode_after_first_byte(stdin_fd, byte).map(|k| k.map(UiEvent::Key))
}

/// Совместимость: события мыши отбрасываются
#[allow(dead_code)]
pub fn read_key(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<Key>> {
    Ok(read_ui_event(stdin_fd)?.and_then(|ev| match ev {
        UiEvent::Key(k) => Some(k),
        UiEvent::Mouse(_) => None,
    }))
}

fn decode_after_first_byte(stdin_fd: std::os::unix::io::RawFd, byte: u8) -> io::Result<Option<Key>> {
    if byte == 127 {
        return Ok(Some(Key::Backspace));
    }

    // Enter обрабатываем по CR; LF (0x0a) оставляем для Ctrl+J.
    if byte == b'\r' {
        return Ok(Some(Key::Enter));
    }

    if byte == 9 {
        return Ok(Some(Key::Tab));
    }

    if byte < 32 {
        if byte == 19 {
            return Ok(Some(Key::CtrlS));
        }

        if byte == 1 {
            return Ok(Some(Key::CtrlA));
        }

        if byte == 20 {
            return Ok(Some(Key::CtrlT));
        }

        if byte == 25 {
            return Ok(Some(Key::CtrlY));
        }

        if byte == 4 {
            return Ok(Some(Key::CtrlD));
        }

        if byte == 5 {
            return Ok(Some(Key::CtrlE));
        }

        if byte == 3 {
            return Ok(Some(Key::CtrlC));
        }

        if byte == 22 {
            return Ok(Some(Key::CtrlV));
        }

        if byte == 6 {
            return Ok(Some(Key::CtrlF));
        }

        if byte == 7 {
            return Ok(Some(Key::CtrlG));
        }

        if byte == 18 {
            return Ok(Some(Key::CtrlR));
        }

        if byte == 17 {
            return Ok(Some(Key::CtrlQ));
        }

        if byte == 2 {
            return Ok(Some(Key::CtrlB));
        }

        if byte == 12 {
            return Ok(Some(Key::CtrlL));
        }

        if byte == 8 {
            return Ok(Some(Key::CtrlH));
        }

        if byte == 10 {
            return Ok(Some(Key::CtrlJ));
        }

        if byte == 11 {
            return Ok(Some(Key::CtrlK));
        }

        if byte == 14 {
            return Ok(Some(Key::CtrlN));
        }

        if byte == 15 {
            return Ok(Some(Key::CtrlO));
        }

        if byte == 21 {
            return Ok(Some(Key::CtrlU));
        }

        if byte == 23 {
            return Ok(Some(Key::CtrlW));
        }

        if byte == 24 {
            return Ok(Some(Key::CtrlX));
        }

        if byte == 26 {
            return Ok(Some(Key::CtrlZ));
        }

        if byte == 28 {
            return Ok(Some(Key::CtrlBackslash));
        }

        if byte == 16 {
            return Ok(Some(Key::CtrlP));
        }

        return Ok(None);
    }

    let needed = utf8_char_len(byte);
    if needed == 1 {
        return Ok(Some(Key::Char(char::from_u32(byte as u32).unwrap_or('\u{fffd}'))));
    }

    let mut buf = [0u8; 4];
    buf[0] = byte;
    for i in 1..needed {
        match read_one_byte(stdin_fd)? {
            Some(b) => buf[i] = b,
            None => return Ok(Some(Key::Char('\u{fffd}'))),
        }
    }

    let ch = std::str::from_utf8(&buf[..needed])
        .ok()
        .and_then(|s| s.chars().next())
        .unwrap_or('\u{fffd}');
    Ok(Some(Key::Char(ch)))
}

fn read_one_byte(fd: std::os::unix::io::RawFd) -> io::Result<Option<u8>> {
    let mut b = [0u8; 1];
    let n = unsafe { libc::read(fd, b.as_mut_ptr().cast(), 1) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }

    if n == 0 {
        return Ok(None);
    }

    Ok(Some(b[0]))
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if (b & 0xe0) == 0xc0 {
        2
    } else if (b & 0xf0) == 0xe0 {
        3
    } else if (b & 0xf8) == 0xf0 {
        4
    } else {
        1
    }
}

/// Разбор CSI SGR-мыши (`[ < Pb ; Px ; Py M|m`), `seq` - без начального ESC
fn try_parse_sgr_mouse(seq: &[u8]) -> Option<MouseEvent> {
    // Минимум: `[<0;0;0M` = 8 байт.
    if seq.len() < 8 || seq[0] != b'[' || seq[1] != b'<' {
        return None;
    }

    let body = &seq[2..];
    let last = *body.last()?;
    if last != b'm' && last != b'M' {
        return None;
    }

    let inner = std::str::from_utf8(&body[..body.len().saturating_sub(1)]).ok()?;
    let mut parts = inner.split(';');
    let pb: u32 = parts.next()?.parse().ok()?;
    let px: u32 = parts.next()?.parse().ok()?;
    let py: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let modifiers = MouseModifiers {
        shift: (pb & 4) != 0,
        alt: (pb & 8) != 0,
        ctrl: (pb & 16) != 0,
    };

    let code = pb & !(4 | 8 | 16);
    let pressed = last == b'M';
    let motion = (code & 32) != 0;
    let base = code & !32;

    let kind = match code {
        64 => MouseEventKind::WheelUp,
        65 => MouseEventKind::WheelDown,
        _ if motion => {
            if pressed && (base & 0b11) == 0 {
                MouseEventKind::LeftDrag
            } else {
                MouseEventKind::Other
            }
        }
        _ if base <= 2 => {
            if pressed {
                match base {
                    0 => MouseEventKind::LeftPress,
                    1 => MouseEventKind::MiddlePress,
                    2 => MouseEventKind::RightPress,
                    _ => MouseEventKind::Other,
                }
            } else {
                MouseEventKind::Release
            }
        }
        3 if !pressed => MouseEventKind::Release,
        _ => MouseEventKind::Other,
    };

    Some(MouseEvent {
        kind,
        column: px.max(1),
        row: py.max(1),
        modifiers,
    })
}

fn parse_escape_event(stdin_fd: std::os::unix::io::RawFd) -> io::Result<Option<UiEvent>> {
    let mut seq = Vec::<u8>::new();
    let mut scratch = [0u8; 64];
    // Первый байт после ESC в некоторых терминалах/tmux может прийти заметно позже
    let mut first_wait = true;
    for _ in 0..8 {
        let timeout_ms = if first_wait { 700 } else { 80 };
        let n = read_timeout(stdin_fd, &mut scratch, timeout_ms)?;
        first_wait = false;
        if n == 0 {
            break;
        }

        seq.extend_from_slice(&scratch[..n]);
        if seq.len() > 96 {
            break;
        }
    }

    if seq.is_empty() {
        return Ok(Some(UiEvent::Key(Key::Esc)));
    }

    // Некоторые терминалы могут добавлять лишний байт ESC в начале
    while seq.first() == Some(&0x1b) {
        seq.remove(0);
    }

    if seq.is_empty() {
        return Ok(Some(UiEvent::Key(Key::Esc)));
    }

    // Дочитываем SGR-мышь до финального `M`/`m`.
    if seq.len() >= 2 && seq[0] == b'[' && seq[1] == b'<' {
        while seq.last() != Some(&b'M') && seq.last() != Some(&b'm') && seq.len() < 128 {
            let n = read_timeout(stdin_fd, &mut scratch, 80)?;
            if n == 0 {
                break;
            }
            seq.extend_from_slice(&scratch[..n]);
        }
        if let Some(m) = try_parse_sgr_mouse(&seq) {
            return Ok(Some(UiEvent::Mouse(m)));
        }
    }

    if seq[0] == b'O' && seq.len() >= 3 && seq[1] == b'5' {
        return Ok(Some(UiEvent::Key(match seq[2] {
            b'D' => Key::CtrlArrowLeft,
            b'C' => Key::CtrlArrowRight,
            b'A' => Key::CtrlArrowUp,
            b'B' => Key::CtrlArrowDown,
            _ => return Ok(None),
        })));
    }

    if seq[0] == b'O' && seq.len() >= 2 {
        return Ok(Some(UiEvent::Key(match seq[1] {
            b'A' => Key::ArrowUp,
            b'B' => Key::ArrowDown,
            b'C' => Key::ArrowRight,
            b'D' => Key::ArrowLeft,
            b'H' => Key::Home,
            b'F' => Key::End,
            _ => return Ok(None),
        })));
    } else if seq[0] == b'O' {
        // Неполная SS3-последовательность: игнорируем эту клавишу
        return Ok(None);
    }

    if seq[0] != b'[' {
        return Ok(None);
    }

    let body = &seq[1..];
    if body.is_empty() {
        // Неполная CSI-последовательность (часто при разбиении стрелок на несколько чтений): игнорируем
        return Ok(None);
    }

    if body[0] == b'Z' {
        return Ok(Some(UiEvent::Key(Key::ShiftTab)));
    }

    if let Some(k) = parse_csi_modified_arrow(body) {
        return Ok(Some(UiEvent::Key(k)));
    }

    match body[0] {
        b'A' => return Ok(Some(UiEvent::Key(Key::ArrowUp))),
        b'B' => return Ok(Some(UiEvent::Key(Key::ArrowDown))),
        b'C' => return Ok(Some(UiEvent::Key(Key::ArrowRight))),
        b'D' => return Ok(Some(UiEvent::Key(Key::ArrowLeft))),
        b'H' => return Ok(Some(UiEvent::Key(Key::Home))),
        b'F' => return Ok(Some(UiEvent::Key(Key::End))),
        _ => {}
    }

    // Обычные CSI-стрелки (без параметров `;`), например ESC [ A, а не ESC [ 1 ; 5 D
    if !body.contains(&b';') {
        if let Some(last) = body.last().copied() {
            match last {
                b'A' => return Ok(Some(UiEvent::Key(Key::ArrowUp))),
                b'B' => return Ok(Some(UiEvent::Key(Key::ArrowDown))),
                b'C' => return Ok(Some(UiEvent::Key(Key::ArrowRight))),
                b'D' => return Ok(Some(UiEvent::Key(Key::ArrowLeft))),
                b'H' => return Ok(Some(UiEvent::Key(Key::Home))),
                b'F' => return Ok(Some(UiEvent::Key(Key::End))),
                _ => {}
            }
        }
    }

    if body.len() >= 2 && *body.last().unwrap_or(&0) == b'~' {
        let lead = body[0];
        match lead {
            b'1' | b'7' => return Ok(Some(UiEvent::Key(Key::Home))),
            b'2' => return Ok(None),
            b'3' => return Ok(Some(UiEvent::Key(Key::Delete))),
            b'4' | b'8' => return Ok(Some(UiEvent::Key(Key::End))),
            b'5' => return Ok(Some(UiEvent::Key(Key::PageUp))),
            b'6' => return Ok(Some(UiEvent::Key(Key::PageDown))),
            _ => {}
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_wheel_up() {
        let m = try_parse_sgr_mouse(b"[<64;12;34M").expect("parse");
        assert_eq!(m.kind, MouseEventKind::WheelUp);
        assert_eq!(m.column, 12);
        assert_eq!(m.row, 34);
    }

    #[test]
    fn sgr_wheel_down_ctrl() {
        let m = try_parse_sgr_mouse(b"[<81;1;2M").expect("parse");
        assert_eq!(m.kind, MouseEventKind::WheelDown);
        assert!(m.modifiers.ctrl);
    }

    #[test]
    fn sgr_left_press() {
        let m = try_parse_sgr_mouse(b"[<0;5;6M").expect("parse");
        assert_eq!(m.kind, MouseEventKind::LeftPress);
        assert_eq!(m.column, 5);
        assert_eq!(m.row, 6);
    }

    #[test]
    fn sgr_left_drag() {
        let m = try_parse_sgr_mouse(b"[<32;7;9M").expect("parse");
        assert_eq!(m.kind, MouseEventKind::LeftDrag);
        assert_eq!(m.column, 7);
        assert_eq!(m.row, 9);
    }
}

/// Стиль `ESC [ 1 ; 5 D` (xterm): последний числовой параметр перед финальным байтом - это модификатор (`5` = Ctrl)
fn parse_csi_modified_arrow(body: &[u8]) -> Option<Key> {
    if body.len() < 3 {
        return None;
    }
    
    let dir = *body.last()?;
    if !matches!(dir, b'A' | b'B' | b'C' | b'D') {
        return None;
    }

    let prefix = &body[..body.len() - 1];
    if !prefix.contains(&b';') {
        return None;
    }

    let modifier = parse_csi_final_modifier(prefix)?;
    match (modifier, dir) {
        (5, b'D') => Some(Key::CtrlArrowLeft),
        (5, b'C') => Some(Key::CtrlArrowRight),
        (5, b'A') => Some(Key::CtrlArrowUp),
        (5, b'B') => Some(Key::CtrlArrowDown),
        _ => None,
    }
}

fn parse_csi_final_modifier(prefix: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(prefix).ok()?;
    let last_seg = s.split(';').last()?;
    last_seg.parse().ok()
}
