#![allow(dead_code)]

mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct LanguagesTexts {
    pub plugin_name: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static LanguagesTexts {
    match lang {
        Language::Ru => &ru::LANGUAGES_RU,
        Language::En => &en::LANGUAGES_EN,
    }
}
