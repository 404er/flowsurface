// i18n! 宏已在 main.rs (crate root) 中初始化
pub use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en-US",
            Language::SimplifiedChinese => "zh-CN",
        }
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::SimplifiedChinese => "简体中文",
        }
    }
    pub fn from_code(code: String) -> Language {
        match code.as_str() {
            "en-US" => Language::English,
            "zh-CN" => Language::SimplifiedChinese,
            _ => Language::English,
        }
    }
}

pub fn set_language(lang: Language) {
    rust_i18n::set_locale(lang.code());
}

pub fn current_language() -> String {
    rust_i18n::locale().to_string()
}