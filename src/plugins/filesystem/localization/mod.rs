#![allow(dead_code)]

mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct FilesystemTexts {
    pub plugin_name: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static FilesystemTexts {
    match lang {
        Language::Ru => &ru::FILESYSTEM_RU,
        Language::En => &en::FILESYSTEM_EN,
    }
}
