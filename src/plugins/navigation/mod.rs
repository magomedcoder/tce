use crate::core::keys::Key;
use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

pub(crate) mod localization;

const QUICK_OPEN: &str = "quick_open";
const PROJECT_SEARCH: &str = "project_search";
const GO_SYMBOL: &str = "go_symbol";
const GO_LINE: &str = "go_line";
const IN_FILE_FIND: &str = "in_file_find";

pub struct NavigationPlugin;

impl WorkspacePlugin for NavigationPlugin {
    fn id(&self) -> &'static str {
        "navigation"
    }

    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        let tx = localization::texts(ws.current_language());
        vec![
            PaletteCommand::new(tx.quick_open_file.to_string(), QUICK_OPEN),
            PaletteCommand::new(tx.search_in_project.to_string(), PROJECT_SEARCH),
            PaletteCommand::new(tx.go_to_symbol.to_string(), GO_SYMBOL),
            PaletteCommand::new(tx.go_to_line.to_string(), GO_LINE),
            PaletteCommand::new(tx.find_in_file.to_string(), IN_FILE_FIND),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            QUICK_OPEN => ws.open_quick_open(),
            PROJECT_SEARCH => ws.open_project_search(),
            GO_SYMBOL => ws.open_symbol_jump(),
            GO_LINE => ws.open_go_to_line(),
            IN_FILE_FIND => ws.open_in_file_find_seeded(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if ws.plugin_is_quick_open_active() {
            ws.plugin_handle_quick_open_key(key);
            return true;
        }
        if ws.plugin_is_in_file_find_active() {
            ws.plugin_handle_in_file_find_key(key);
            return true;
        }
        if ws.plugin_is_project_search_active() {
            ws.plugin_handle_project_search_key(key);
            return true;
        }
        if ws.plugin_is_symbol_jump_active() {
            ws.plugin_handle_symbol_jump_key(key);
            return true;
        }
        if ws.plugin_is_go_to_line_active() {
            ws.plugin_handle_go_to_line_key(key);
            return true;
        }

        match key {
            Key::CtrlO => ws.open_quick_open(),
            Key::CtrlF => ws.open_project_search(),
            Key::CtrlBackslash => ws.open_in_file_find_seeded(),
            Key::CtrlT => ws.open_symbol_jump(),
            Key::CtrlY => ws.open_go_to_line(),
            Key::CtrlA => ws.plugin_navigate_back(),
            Key::CtrlZ => ws.plugin_navigate_forward(),
            _ => return false,
        }
        true
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_navigation_overlays(out, cols, rows)
    }
}
