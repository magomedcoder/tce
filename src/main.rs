#[cfg(unix)]
mod buffer;
#[cfg(unix)]
mod editor;
#[cfg(unix)]
mod terminal;

#[cfg(not(unix))]
fn main() {
    eprintln!("Tce - Terminal code editor");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    match editor::Editor::new(path) {
        Ok(mut ed) => {
            if let Err(e) = ed.run() {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}