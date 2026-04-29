use crate::workspace::Workspace;
use crate::core::keys::Key;

#[derive(Clone, Debug)]
pub struct PaletteCommand {
    pub title: String,
    pub id: String,
}

impl PaletteCommand {
    pub fn new(title: String, id: impl Into<String>) -> Self {
        Self {
            title,
            id: id.into(),
        }
    }
}

pub trait WorkspacePlugin {
    fn id(&self) -> &'static str;
    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand>;
    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool;
    fn handle_key(&self, _ws: &mut Workspace, _key: Key) -> bool {
        false
    }
    fn post_handle_key(&self, _ws: &mut Workspace, _key: Key) {}
    fn render_overlay(
        &self,
        _ws: &Workspace,
        _out: &mut String,
        _cols: usize,
        _rows: usize,
    ) -> bool {
        false
    }
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn WorkspacePlugin>>,
}

impl PluginRegistry {
    pub fn new(plugins: Vec<Box<dyn WorkspacePlugin>>) -> Self {
        Self { plugins }
    }

    pub fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            let _plugin_id = plugin.id();
            out.extend(plugin.palette_commands(ws));
        }
        out
    }

    pub fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        for plugin in &self.plugins {
            if plugin.run_command(ws, cmd) {
                return true;
            }
        }
        false
    }

    pub fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        for plugin in &self.plugins {
            if plugin.handle_key(ws, key) {
                return true;
            }
        }
        false
    }

    pub fn post_handle_key(&self, ws: &mut Workspace, key: Key) {
        for plugin in &self.plugins {
            plugin.post_handle_key(ws, key);
        }
    }

    pub fn render_overlay(
        &self,
        ws: &Workspace,
        out: &mut String,
        cols: usize,
        rows: usize,
    ) -> bool {
        for plugin in &self.plugins {
            if plugin.render_overlay(ws, out, cols, rows) {
                return true;
            }
        }
        false
    }
}
