use std::io;

use crate::core::keys::{Key, MouseEvent};

/// Базовый контракт фазы UI-жизненного цикла в ядре
/// Фаза обязана уметь отрисоваться и обработать пользовательские события
#[allow(dead_code)]
pub trait CorePhase {
    fn render(&mut self) -> io::Result<()>;
    fn handle_key(&mut self, key: Key) -> io::Result<PhaseTransition>;
    fn handle_mouse(&mut self, _mouse: MouseEvent) -> io::Result<PhaseTransition> {
        Ok(PhaseTransition::Stay)
    }
}

/// Результат обработки события в фазе жизненного цикла
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum PhaseTransition {
    Stay,
    Quit,
}

/// Примитивы рендера/режима терминала, используемые event loop ядра
pub struct RenderingPrimitives;

impl RenderingPrimitives {
    pub const ENTER: &'static str = "\x1b[?25l\x1b[?7l\x1b[?1000h\x1b[?1002h\x1b[?1006h";
    pub const LEAVE: &'static str = "\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?25h\x1b[?7h\x1b[m\r\n";
}
