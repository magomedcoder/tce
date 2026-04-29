pub(crate) mod core_ui;
mod diagnostics;
pub(crate) mod filesystem;
mod git;
pub(crate) mod llm;
pub(crate) mod languages;
mod navigation;

use crate::core::plugin::{PluginRegistry, WorkspacePlugin};

pub fn builtin_registry() -> PluginRegistry {
    let plugins: Vec<Box<dyn WorkspacePlugin>> = vec![
        Box::new(core_ui::CoreUiPlugin),
        Box::new(filesystem::FilesystemPlugin),
        Box::new(diagnostics::DiagnosticsPlugin),
        Box::new(navigation::NavigationPlugin),
        Box::new(git::GitPlugin),
        Box::new(llm::LlmPlugin),
    ];
    PluginRegistry::new(plugins)
}
