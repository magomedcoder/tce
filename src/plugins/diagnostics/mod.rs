use crate::core::keys::Key;
use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

pub(crate) mod localization;

const RUST_CHECK_CURRENT: &str = "rust_check_current";
const SHOW_DIAGNOSTICS: &str = "show_diagnostics";

pub struct DiagnosticsPlugin;

impl WorkspacePlugin for DiagnosticsPlugin {
    fn id(&self) -> &'static str {
        "diagnostics"
    }

    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        let tx = localization::texts(ws.current_language());
        vec![
            PaletteCommand::new(tx.rust_check_current.to_string(), RUST_CHECK_CURRENT),
            PaletteCommand::new(tx.show_diagnostics.to_string(), SHOW_DIAGNOSTICS),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            RUST_CHECK_CURRENT => ws.plugin_run_rust_check_current_file(),
            SHOW_DIAGNOSTICS => ws.plugin_show_diagnostics(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if !ws.plugin_is_diagnostics_open() {
            return false;
        }
        ws.plugin_handle_diagnostics_key(key);
        true
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_diagnostics_overlay(out, cols, rows)
    }
}
