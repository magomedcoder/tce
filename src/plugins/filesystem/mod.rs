use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::plugin::WorkspacePlugin;
use crate::core::keys::Key;
use crate::workspace::Workspace;

pub(crate) mod localization;

const MAX_ENTRIES: usize = 4000;

static SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cargo",
    "__pycache__",
    ".idea",
    ".vscode",
];

#[derive(Clone, Debug)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub label: String,
}

pub fn build_tree(root: &Path) -> io::Result<Vec<TreeEntry>> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }

    walk(root, 0, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<TreeEntry>) -> io::Result<()> {
    if out.len() >= MAX_ENTRIES {
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by(|a, b| {
        let ad = a.path().is_dir();
        let bd = b.path().is_dir();
        ad.cmp(&bd)
            .reverse()
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });

    for e in entries {
        if out.len() >= MAX_ENTRIES {
            break;
        }

        let name = e.file_name();
        if should_skip_dir(&name) {
            continue;
        }

        let p = e.path();
        let is_dir = p.is_dir();
        let label = name.to_string_lossy().into_owned();
        out.push(TreeEntry {
            path: p.clone(),
            depth,
            is_dir,
            label,
        });
        if is_dir {
            walk(&p, depth + 1, out)?;
        }
    }
    Ok(())
}

fn should_skip_dir(name: &OsStr) -> bool {
    SKIP_DIR_NAMES
        .iter()
        .any(|s| Some(OsStr::new(s)) == Some(name))
}

pub struct FilesystemPlugin;

impl WorkspacePlugin for FilesystemPlugin {
    fn id(&self) -> &'static str {
        "filesystem"
    }

    fn palette_commands(&self, _ws: &Workspace) -> Vec<crate::core::plugin::PaletteCommand> {
        Vec::new()
    }

    fn run_command(&self, _ws: &mut Workspace, _cmd: &str) -> bool {
        false
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if ws.plugin_is_sidebar_prompt_active() {
            ws.plugin_handle_sidebar_prompt_key(key);
            return true;
        }
        if ws.plugin_is_sidebar_menu_open() {
            ws.plugin_handle_sidebar_menu_key(key);
            return true;
        }
        if ws.plugin_is_sidebar_focused() {
            ws.plugin_handle_sidebar_key(key);
            return true;
        }
        false
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_filesystem_overlays(out, cols, rows)
    }
}
