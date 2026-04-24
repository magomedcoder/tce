//! Recent project roots (directories), one path per line in ~/.config/.tce/recents.txt

use std::fs;
use std::io;
use std::path::PathBuf;

const MAX_RECENTS: usize = 24;
const FILE_NAME: &str = "recents.txt";

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join(".config")
            .join(".tce")
    })
}

fn recents_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(FILE_NAME))
}

pub fn load() -> Vec<PathBuf> {
    let Some(p) = recents_path() else {
        return Vec::new();
    };

    let Ok(s) = fs::read_to_string(&p) else {
        return Vec::new();
    };

    let mut out: Vec<PathBuf> = Vec::new();
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        let pb = PathBuf::from(t);
        if pb.is_dir() {
            out.push(pb);
        }
    }

    dedupe_keep_order(&mut out);
    out.truncate(MAX_RECENTS);
    out
}

pub fn push_front(root: PathBuf) -> io::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }

    let Ok(canonical) = root.canonicalize() else {
        return Ok(());
    };

    let Some(dir) = config_dir() else {
        return Ok(());
    };

    fs::create_dir_all(&dir)?;
    let path = dir.join(FILE_NAME);
    let mut list = load();
    list.retain(|p| p != &canonical);
    list.insert(0, canonical);
    list.truncate(MAX_RECENTS);
    dedupe_keep_order(&mut list);
    let body: String = list
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        fs::write(&path, "")?;
    } else {
        fs::write(&path, format!("{body}\n"))?;
    }
    Ok(())
}

fn dedupe_keep_order(paths: &mut Vec<PathBuf>) {
    let mut seen = std::collections::HashSet::<String>::new();
    paths.retain(|p| {
        let k = p.to_string_lossy().to_string();
        seen.insert(k)
    });
}
