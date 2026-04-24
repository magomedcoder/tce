mod en;
mod ru;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Ru,
    En,
}

pub struct Texts {
    pub welcome_title: &'static str,
    pub welcome_recents: &'static str,
    pub welcome_empty: &'static str,
    pub welcome_open_new_quit: &'static str,
    pub welcome_folders: &'static str,
    pub welcome_folder_hint: &'static str,
    pub welcome_folder_open_current: &'static str,
    pub welcome_folder_home: &'static str,
    pub welcome_folder_root: &'static str,
    pub welcome_folder_up: &'static str,
    pub welcome_manual_path_hint: &'static str,
    pub welcome_path_prompt: &'static str,
    pub welcome_status_pos: &'static str,
    pub welcome_status_enter_esc: &'static str,
    pub hint_sidebar_focus: &'static str,
    pub hint_ctrl_b: &'static str,
    pub hint_shift_tab: &'static str,
    pub hint_ctrl_q_quit: &'static str,
    pub hint_ctrl_q_again_quit: &'static str,
    pub hint_ctrl_s_save: &'static str,
    pub hint_ctrl_l_lang: &'static str,
    pub hint_ctrl_k_help: &'static str,
    pub language_menu_title: &'static str,
    pub language_menu_hint: &'static str,
    pub language_option_en: &'static str,
    pub language_option_ru: &'static str,
    pub help_title: &'static str,
    pub help_hint: &'static str,
    pub help_k1: &'static str,
    pub help_k2: &'static str,
    pub help_k3: &'static str,
    pub help_k4: &'static str,
    pub help_k5: &'static str,
    pub help_k6: &'static str,
    pub help_k7: &'static str,
    pub save_or_quit_double: &'static str,
    pub error_prefix: &'static str,
}

pub fn texts(lang: Language) -> &'static Texts {
    match lang {
        Language::Ru => &ru::TEXTS_RU,
        Language::En => &en::TEXTS_EN,
    }
}
