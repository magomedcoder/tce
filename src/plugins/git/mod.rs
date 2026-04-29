use crate::core::keys::Key;
use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

pub(crate) mod localization;

const STATUS: &str = "git_status";
const DIFF_UNSTAGED: &str = "git_diff_unstaged";
const DIFF_STAGED: &str = "git_diff_staged";
const LOG: &str = "git_log";

pub struct GitPlugin;

impl WorkspacePlugin for GitPlugin {
    fn id(&self) -> &'static str {
        "git"
    }

    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        let tx = localization::texts(ws.current_language());
        vec![
            PaletteCommand::new(tx.status.to_string(), STATUS),
            PaletteCommand::new(tx.diff_unstaged.to_string(), DIFF_UNSTAGED),
            PaletteCommand::new(tx.diff_staged.to_string(), DIFF_STAGED),
            PaletteCommand::new(tx.recent_commits.to_string(), LOG),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            STATUS => ws.plugin_open_git_status(),
            DIFF_UNSTAGED => ws.plugin_open_git_diff_unstaged(),
            DIFF_STAGED => ws.plugin_open_git_diff_staged(),
            LOG => ws.plugin_open_git_log(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if ws.plugin_is_git_view_open() {
            ws.plugin_handle_git_view_key(key);
            return true;
        }
        false
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_git_overlay(out, cols, rows)
    }
}
