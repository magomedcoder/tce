use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

const HOVER: &str = "lsp_hover";
const GO_TO_DEFINITION: &str = "lsp_go_to_definition";
const REFERENCES: &str = "lsp_references";
const RENAME: &str = "lsp_rename";
const CODE_ACTIONS: &str = "lsp_code_actions";

pub struct LspPlugin;

impl WorkspacePlugin for LspPlugin {
    fn id(&self) -> &'static str {
        "lsp"
    }

    fn palette_commands(&self, _ws: &Workspace) -> Vec<PaletteCommand> {
        vec![
            PaletteCommand::new("LSP: Hover".to_string(), HOVER),
            PaletteCommand::new("LSP: Go to definition".to_string(), GO_TO_DEFINITION),
            PaletteCommand::new("LSP: Find references".to_string(), REFERENCES),
            PaletteCommand::new("LSP: Rename symbol".to_string(), RENAME),
            PaletteCommand::new("LSP: Code actions".to_string(), CODE_ACTIONS),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            HOVER => ws.plugin_lsp_hover(),
            GO_TO_DEFINITION => ws.plugin_lsp_go_to_definition(),
            REFERENCES => ws.plugin_lsp_references(),
            RENAME => ws.plugin_lsp_rename_symbol(),
            CODE_ACTIONS => ws.plugin_lsp_code_actions(),
            _ => return false,
        }
        true
    }
}
