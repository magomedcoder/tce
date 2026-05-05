use std::fs;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::core::keys::{read_ui_event, UiEvent};
use crate::core::lifecycle::{CorePhase, PhaseTransition, RenderingPrimitives};
use crate::core::terminal::RawMode;
use crate::plugins::core_ui::welcome::{Welcome, WelcomeAction};
use crate::workspace::Workspace;

enum Phase {
    Welcome(Welcome),
    Workspace(Workspace),
}

pub struct App {
    phase: Phase,
}

impl App {
    pub fn from_args(arg: Option<PathBuf>) -> io::Result<Self> {
        let phase = match arg {
            None => Phase::Welcome(Welcome::new()),
            Some(p) => {
                match fs::metadata(&p) {
                    Ok(meta) => {
                        if meta.is_dir() {
                            Phase::Workspace(Workspace::open_dir(p)?)
                        } else {
                            Phase::Workspace(Workspace::open_file_in_project(p)?)
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        // Разрешаем запуск с новым файлом:
                        // `tce path/to/new_file.rs` открывает пустой буфер с привязкой к пути.
                        Phase::Workspace(Workspace::open_file_in_project(p)?)
                    }
                    Err(err) => return Err(err),
                }
            }
        };
        Ok(Self { phase })
    }

    pub fn run(&mut self) -> io::Result<()> {
        let _raw = RawMode::enable_stdin()?;
        let stdin_fd = std::io::stdin().as_raw_fd();

        write!(io::stdout(), "{}", RenderingPrimitives::ENTER)?;
        io::stdout().flush()?;

        let result = (|| -> io::Result<()> {
            loop {
                match &mut self.phase {
                    Phase::Welcome(w) => {
                        w.render()?;
                        let key = match read_ui_event(stdin_fd)? {
                            None => continue,
                            Some(UiEvent::Mouse(_)) => continue,
                            Some(UiEvent::Key(k)) => k,
                        };

                        match w.handle_key(key)? {
                            WelcomeAction::Quit => break,
                            WelcomeAction::OpenProject(root, language) => {
                                let mut ws = Workspace::open_project(root)?;
                                ws.set_language(language);
                                self.phase = Phase::Workspace(ws);
                            }
                            WelcomeAction::None => {}
                        }
                    }
                    Phase::Workspace(ws) => {
                        ws.render()?;
                        match read_ui_event(stdin_fd)? {
                            None => continue,
                            Some(UiEvent::Mouse(m)) => {
                                if ws.handle_mouse(m)? {
                                    break;
                                }
                            }
                            Some(UiEvent::Key(k)) => {
                                if ws.handle_key(k)? {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })();

        write!(io::stdout(), "{}", RenderingPrimitives::LEAVE)?;
        io::stdout().flush()?;

        result
    }
}

impl CorePhase for Workspace {
    fn render(&mut self) -> io::Result<()> {
        Workspace::render(self)
    }

    fn handle_key(&mut self, key: crate::core::keys::Key) -> io::Result<PhaseTransition> {
        if Workspace::handle_key(self, key)? {
            Ok(PhaseTransition::Quit)
        } else {
            Ok(PhaseTransition::Stay)
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: crate::core::keys::MouseEvent,
    ) -> io::Result<PhaseTransition> {
        if Workspace::handle_mouse(self, mouse)? {
            Ok(PhaseTransition::Quit)
        } else {
            Ok(PhaseTransition::Stay)
        }
    }
}

impl CorePhase for Welcome {
    fn render(&mut self) -> io::Result<()> {
        Welcome::render(self)
    }

    fn handle_key(&mut self, key: crate::core::keys::Key) -> io::Result<PhaseTransition> {
        match Welcome::handle_key(self, key)? {
            WelcomeAction::Quit => Ok(PhaseTransition::Quit),
            WelcomeAction::OpenProject(_, _) | WelcomeAction::None => Ok(PhaseTransition::Stay),
        }
    }
}
