mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct CoreUiTexts {
    pub toggle_sidebar: &'static str,
    pub toggle_right_panel: &'static str,
    pub toggle_theme: &'static str,
    pub toggle_autosave: &'static str,
    pub increase_font: &'static str,
    pub decrease_font: &'static str,
    pub toggle_line_spacing: &'static str,
    pub toggle_ligatures: &'static str,
    pub toggle_pin_tab: &'static str,
    pub show_hotkeys: &'static str,
    pub language_picker: &'static str,
    pub lsp_wave: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static CoreUiTexts {
    match lang {
        Language::Ru => &ru::CORE_UI_RU,
        Language::En => &en::CORE_UI_EN,
    }
}
