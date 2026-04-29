mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct NavigationTexts {
    pub quick_open_file: &'static str,
    pub search_in_project: &'static str,
    pub go_to_symbol: &'static str,
    pub go_to_line: &'static str,
    pub find_in_file: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static NavigationTexts {
    match lang {
        Language::Ru => &ru::NAV_RU,
        Language::En => &en::NAV_EN,
    }
}
