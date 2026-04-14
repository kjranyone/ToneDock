use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    En,
    Ja,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::En, Language::Ja];

    #[allow(dead_code)]
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Ja => "ja",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Ja => "日本語",
        }
    }

    #[allow(dead_code)]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Language::En),
            "ja" => Some(Language::Ja),
            _ => None,
        }
    }

    pub fn from_system_locale() -> Self {
        let lang = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .or_else(|_| std::env::var("LANGUAGE"))
            .unwrap_or_default()
            .to_lowercase();

        let sys_lang = sys_locale::get_locale().unwrap_or_default().to_lowercase();

        if lang.starts_with("ja") || sys_lang.starts_with("ja") {
            Language::Ja
        } else {
            Language::En
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::from_system_locale()
    }
}

pub struct I18n {
    lang: Language,
    strings: HashMap<String, String>,
}

impl I18n {
    pub fn new(lang: Language) -> Self {
        let json_str = match lang {
            Language::En => include_str!("../locales/en.json"),
            Language::Ja => include_str!("../locales/ja.json"),
        };
        let strings: HashMap<String, String> = serde_json::from_str(json_str).unwrap_or_else(|e| {
            log::error!("Failed to parse locale {:?}: {}", lang, e);
            HashMap::new()
        });
        Self { lang, strings }
    }

    pub fn language(&self) -> Language {
        self.lang
    }

    pub fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map(|s| s.as_str()).unwrap_or(key)
    }

    pub fn trf(&self, key: &str, args: &[(&str, &str)]) -> String {
        let template = self.strings.get(key).map(|s| s.as_str()).unwrap_or(key);
        let mut result = template.to_string();
        for (name, value) in args {
            result = result.replace(&format!("{{{}}}", name), value);
        }
        result
    }
}
