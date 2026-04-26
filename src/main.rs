#[cfg(unix)]
mod app;
#[cfg(unix)]
mod buffer;
#[cfg(unix)]
mod document;
#[cfg(unix)]
mod keys;
#[cfg(unix)]
mod languages;
#[cfg(unix)]
mod localization;
#[cfg(unix)]
mod recents;
#[cfg(unix)]
mod session;
#[cfg(unix)]
mod settings;
#[cfg(unix)]
mod terminal;
#[cfg(unix)]
mod tree;
#[cfg(unix)]
mod welcome;
#[cfg(unix)]
mod workspace;

#[cfg(not(unix))]
fn main() {
    eprintln!("Tce - Terminal code editor");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    match app::App::from_args(path) {
        Ok(mut app) => {
            if let Err(e) = app.run() {
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
