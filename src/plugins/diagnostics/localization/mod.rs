mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct DiagnosticsTexts {
    pub rust_check_current: &'static str,
    pub show_diagnostics: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static DiagnosticsTexts {
    match lang {
        Language::Ru => &ru::DIAG_RU,
        Language::En => &en::DIAG_EN,
    }
}
