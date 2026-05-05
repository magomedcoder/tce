use crate::core::keys::Key;
use crate::core::plugin::WorkspacePlugin;
use crate::workspace::Workspace;

pub struct EditingPlugin;

impl WorkspacePlugin for EditingPlugin {
    fn id(&self) -> &'static str {
        "editing"
    }

    fn palette_commands(&self, _ws: &Workspace) -> Vec<crate::core::plugin::PaletteCommand> {
        Vec::new()
    }

    fn run_command(&self, _ws: &mut Workspace, _cmd: &str) -> bool {
        false
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        match key {
            Key::CtrlD => ws.plugin_start_multi_edit_from_cursor(),
            Key::CtrlE => ws.plugin_start_sync_edit(),
            _ => return false,
        }
        true
    }
}
