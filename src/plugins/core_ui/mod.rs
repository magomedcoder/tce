use crate::core::keys::Key;
use crate::core::plugin::{PaletteCommand, WorkspacePlugin};
use crate::workspace::Workspace;

pub(crate) mod localization;
pub(crate) mod welcome;

const TOGGLE_SIDEBAR: &str = "toggle_sidebar";
const TOGGLE_RIGHT_PANEL: &str = "toggle_right_panel";
const TOGGLE_THEME: &str = "toggle_theme";
const TOGGLE_AUTOSAVE: &str = "toggle_autosave";
const FONT_PLUS: &str = "font_plus";
const FONT_MINUS: &str = "font_minus";
const TOGGLE_LINE_SPACING: &str = "toggle_line_spacing";
const TOGGLE_LIGATURES: &str = "toggle_ligatures";
const TOGGLE_PIN: &str = "toggle_pin";
const SHOW_HELP: &str = "show_help";
const LANGUAGE_PICKER: &str = "language_picker";
const LSP_WAVE_EXTENSIONS: &str = "lsp_wave_extensions";

pub struct CoreUiPlugin;

impl WorkspacePlugin for CoreUiPlugin {
    fn id(&self) -> &'static str {
        "core_ui"
    }

    fn palette_commands(&self, ws: &Workspace) -> Vec<PaletteCommand> {
        let tx = localization::texts(ws.current_language());
        vec![
            PaletteCommand::new(tx.toggle_sidebar.to_string(), TOGGLE_SIDEBAR),
            PaletteCommand::new(tx.toggle_right_panel.to_string(), TOGGLE_RIGHT_PANEL),
            PaletteCommand::new(tx.toggle_theme.to_string(), TOGGLE_THEME),
            PaletteCommand::new(tx.toggle_autosave.to_string(), TOGGLE_AUTOSAVE),
            PaletteCommand::new(tx.increase_font.to_string(), FONT_PLUS),
            PaletteCommand::new(tx.decrease_font.to_string(), FONT_MINUS),
            PaletteCommand::new(tx.toggle_line_spacing.to_string(), TOGGLE_LINE_SPACING),
            PaletteCommand::new(tx.toggle_ligatures.to_string(), TOGGLE_LIGATURES),
            PaletteCommand::new(tx.toggle_pin_tab.to_string(), TOGGLE_PIN),
            PaletteCommand::new(tx.show_hotkeys.to_string(), SHOW_HELP),
            PaletteCommand::new(tx.language_picker.to_string(), LANGUAGE_PICKER),
            PaletteCommand::new(tx.lsp_wave.to_string(), LSP_WAVE_EXTENSIONS),
        ]
    }

    fn run_command(&self, ws: &mut Workspace, cmd: &str) -> bool {
        match cmd {
            TOGGLE_SIDEBAR => ws.plugin_toggle_sidebar(),
            TOGGLE_RIGHT_PANEL => ws.plugin_toggle_right_panel(),
            TOGGLE_THEME => ws.plugin_toggle_theme(),
            TOGGLE_AUTOSAVE => ws.plugin_toggle_autosave(),
            FONT_PLUS => ws.plugin_increase_font(),
            FONT_MINUS => ws.plugin_decrease_font(),
            TOGGLE_LINE_SPACING => ws.plugin_toggle_line_spacing(),
            TOGGLE_LIGATURES => ws.plugin_toggle_ligatures(),
            TOGGLE_PIN => ws.plugin_toggle_pin_active_tab(),
            SHOW_HELP => ws.plugin_show_hotkeys_help(),
            LANGUAGE_PICKER => ws.plugin_open_language_picker(),
            LSP_WAVE_EXTENSIONS => ws.plugin_show_lsp_wave_extensions(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        match key {
            Key::CtrlB => ws.plugin_toggle_sidebar(),
            Key::CtrlR => ws.plugin_toggle_right_panel(),
            Key::CtrlL => ws.plugin_open_language_picker(),
            Key::CtrlH => ws.plugin_show_hotkeys_help(),
            _ => return false,
        }
        true
    }
}
