mod en;
mod ru;

use crate::localization::Language;

pub(crate) struct GitTexts {
    pub status: &'static str,
    pub diff_unstaged: &'static str,
    pub diff_staged: &'static str,
    pub recent_commits: &'static str,
}

pub(crate) fn texts(lang: Language) -> &'static GitTexts {
    match lang {
        Language::Ru => &ru::GIT_RU,
        Language::En => &en::GIT_EN,
    }
}
