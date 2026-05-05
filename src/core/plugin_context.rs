use crate::core::keys::Key;
use crate::workspace::Workspace;

/// Единая точка доступа плагинов к состоянию и командам Workspace
pub struct PluginContext<'a> {
    ws: &'a mut Workspace,
}

impl<'a> PluginContext<'a> {
    pub fn new(ws: &'a mut Workspace) -> Self {
        Self { ws }
    }

    pub fn run_lint_current_file(&mut self) {
        self.ws.plugin_run_rust_check_current_file();
    }

    pub fn show_diagnostics(&mut self) {
        self.ws.plugin_show_diagnostics();
    }

    pub fn is_diagnostics_open(&self) -> bool {
        self.ws.plugin_is_diagnostics_open()
    }

    pub fn handle_diagnostics_key(&mut self, key: Key) {
        self.ws.plugin_handle_diagnostics_key(key);
    }
}
