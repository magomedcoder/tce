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
const MANAGE_PLUGINS: &str = "manage_plugins";

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
            PaletteCommand::new("Plugins: Manage".to_string(), MANAGE_PLUGINS),
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
            MANAGE_PLUGINS => ws.plugin_open_plugin_manager(),
            _ => return false,
        }
        true
    }

    fn handle_key(&self, ws: &mut Workspace, key: Key) -> bool {
        if ws.plugin_is_plugin_manager_active() {
            ws.plugin_handle_plugin_manager_key(key);
            return true;
        }
        
        if ws.plugin_is_completion_active() {
            ws.plugin_handle_completion_key(key);
            return true;
        }

        if ws.plugin_is_command_palette_active() {
            ws.plugin_handle_command_palette_key(key);
            return true;
        }

        if ws.plugin_is_tabs_focused() {
            ws.plugin_handle_tabs_key(key);
            return true;
        }

        match key {
            Key::CtrlS => ws.plugin_save_active_document(),
            Key::CtrlB => ws.plugin_toggle_sidebar(),
            Key::CtrlR => ws.plugin_toggle_right_panel(),
            Key::CtrlL => ws.plugin_open_language_picker(),
            Key::CtrlH => ws.plugin_show_hotkeys_help(),
            Key::CtrlJ if !ws.plugin_is_tabs_focused() => ws.plugin_open_command_palette(),
            Key::CtrlP => ws.plugin_next_tab(),
            Key::CtrlU => ws.plugin_prev_tab(),
            Key::CtrlW => ws.plugin_close_active_tab(),
            Key::CtrlX => ws.plugin_toggle_pin_active_tab(),
            Key::CtrlArrowLeft if ws.plugin_is_tabs_focused() => ws.plugin_move_tab_left(),
            Key::CtrlArrowRight if ws.plugin_is_tabs_focused() => ws.plugin_move_tab_right(),
            Key::ShiftTab => ws.plugin_focus_prev(),
            Key::Tab => ws.plugin_focus_next(),
            _ => return false,
        }
        true
    }

    fn render_overlay(&self, ws: &Workspace, out: &mut String, cols: usize, rows: usize) -> bool {
        ws.plugin_render_core_ui_overlays(out, cols, rows)
    }
}
