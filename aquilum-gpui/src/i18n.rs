//! Scalable localization (i18n) system for Aquilum / Vibecodium.
//!
//! - Support for 20 interface languages.
//! - All translations are compiled into the binary via `include_str!`.
//! - Lazy initialization into a flat hash table for O(1) lookup.
//! - Multi-level fallback: `requested language` -> `en` -> `key name`.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Information about a supported language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleInfo {
    pub code: &'static str,
    pub native_name: &'static str,
    pub english_name: &'static str,
}

/// List of all 20 supported interface languages.
pub static SUPPORTED_LOCALES: &[LocaleInfo] = &[
    LocaleInfo { code: "ru", native_name: "Русский", english_name: "Russian" },
    LocaleInfo { code: "en", native_name: "English", english_name: "English" },
    LocaleInfo { code: "pl", native_name: "Polski", english_name: "Polish" },
    LocaleInfo { code: "de", native_name: "Deutsch", english_name: "German" },
    LocaleInfo { code: "fr", native_name: "Français", english_name: "French" },
    LocaleInfo { code: "es", native_name: "Español", english_name: "Spanish" },
    LocaleInfo { code: "it", native_name: "Italiano", english_name: "Italian" },
    LocaleInfo { code: "pt", native_name: "Português", english_name: "Portuguese" },
    LocaleInfo { code: "zh-CN", native_name: "简体中文", english_name: "Simplified Chinese" },
    LocaleInfo { code: "zh-TW", native_name: "繁體中文", english_name: "Traditional Chinese" },
    LocaleInfo { code: "ja", native_name: "日本語", english_name: "Japanese" },
    LocaleInfo { code: "ko", native_name: "한국어", english_name: "Korean" },
    LocaleInfo { code: "tr", native_name: "Türkçe", english_name: "Turkish" },
    LocaleInfo { code: "nl", native_name: "Nederlands", english_name: "Dutch" },
    LocaleInfo { code: "cs", native_name: "Čeština", english_name: "Czech" },
    LocaleInfo { code: "sv", native_name: "Svenska", english_name: "Swedish" },
    LocaleInfo { code: "uk", native_name: "Українська", english_name: "Ukrainian" },
    LocaleInfo { code: "ar", native_name: "العربية", english_name: "Arabic" },
    LocaleInfo { code: "vi", native_name: "Tiếng Việt", english_name: "Vietnamese" },
    LocaleInfo { code: "hi", native_name: "हिन्दी", english_name: "Hindi" },
];

impl LocaleInfo {
    /// Looks up locale information by code (case-insensitive).
    pub fn for_code(code: &str) -> Option<&'static LocaleInfo> {
        let code_clean = code.trim();
        SUPPORTED_LOCALES.iter().find(|l| l.code.eq_ignore_ascii_case(code_clean))
    }

    /// Normalizes a language code (with prefix fallback, e.g. "ru_RU" -> "ru").
    pub fn normalize(code: &str) -> &'static str {
        if let Some(info) = Self::for_code(code) {
            return info.code;
        }
        let prefix = code.split(['-', '_']).next().unwrap_or(code).trim();
        for loc in SUPPORTED_LOCALES {
            if loc.code.eq_ignore_ascii_case(prefix) {
                return loc.code;
            }
        }
        "ru"
    }
}

type LocaleMap = HashMap<String, String>;
type RegistryMap = HashMap<&'static str, LocaleMap>;

static REGISTRY: OnceLock<RegistryMap> = OnceLock::new();

fn flatten_json(prefix: &str, val: &serde_json::Value, out: &mut LocaleMap) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_json(&p, v, out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

fn parse_locale_raw(raw: &str) -> LocaleMap {
    let mut map = HashMap::new();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        flatten_json("", &v, &mut map);
    }
    map
}

fn registry() -> &'static RegistryMap {
    REGISTRY.get_or_init(|| {
        let mut reg = HashMap::new();
        reg.insert("ar", parse_locale_raw(include_str!("../locales/ar.json")));
        reg.insert("cs", parse_locale_raw(include_str!("../locales/cs.json")));
        reg.insert("de", parse_locale_raw(include_str!("../locales/de.json")));
        reg.insert("en", parse_locale_raw(include_str!("../locales/en.json")));
        reg.insert("es", parse_locale_raw(include_str!("../locales/es.json")));
        reg.insert("fr", parse_locale_raw(include_str!("../locales/fr.json")));
        reg.insert("hi", parse_locale_raw(include_str!("../locales/hi.json")));
        reg.insert("it", parse_locale_raw(include_str!("../locales/it.json")));
        reg.insert("ja", parse_locale_raw(include_str!("../locales/ja.json")));
        reg.insert("ko", parse_locale_raw(include_str!("../locales/ko.json")));
        reg.insert("nl", parse_locale_raw(include_str!("../locales/nl.json")));
        reg.insert("pl", parse_locale_raw(include_str!("../locales/pl.json")));
        reg.insert("pt", parse_locale_raw(include_str!("../locales/pt.json")));
        reg.insert("ru", parse_locale_raw(include_str!("../locales/ru.json")));
        reg.insert("sv", parse_locale_raw(include_str!("../locales/sv.json")));
        reg.insert("tr", parse_locale_raw(include_str!("../locales/tr.json")));
        reg.insert("uk", parse_locale_raw(include_str!("../locales/uk.json")));
        reg.insert("vi", parse_locale_raw(include_str!("../locales/vi.json")));
        reg.insert("zh-CN", parse_locale_raw(include_str!("../locales/zh-CN.json")));
        reg.insert("zh-TW", parse_locale_raw(include_str!("../locales/zh-TW.json")));
        reg
    })
}

/// Gets a translation by key for the given language.
///
/// Lookup chain:
/// 1. `lang`
/// 2. `en` (fallback)
/// 3. `key` (if the translation is missing)
pub fn t(key: &str, lang: &str) -> String {
    let reg = registry();
    let norm = LocaleInfo::normalize(lang);

    // 1. Look in the requested language
    if let Some(map) = reg.get(norm) {
        if let Some(val) = map.get(key) {
            if !val.is_empty() {
                return val.clone();
            }
        }
    }

    // 2. Fallback to English
    if norm != "en" {
        if let Some(en_map) = reg.get("en") {
            if let Some(val) = en_map.get(key) {
                if !val.is_empty() {
                    return val.clone();
                }
            }
        }
    }

    // 3. Fallback to the key itself
    key.to_string()
}

/// Gets a translation with named parameter substitution `{name}`.
pub fn t_args(key: &str, lang: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key, lang);
    for (name, val) in args {
        s = s.replace(&format!("{{{name}}}"), val);
    }
    s
}

/// Gets a translation with sequential substitution of positional `{}`.
pub fn t_fmt(key: &str, lang: &str, args: &[&str]) -> String {
    let mut s = t(key, lang);
    for arg in args {
        if let Some(pos) = s.find("{}") {
            s.replace_range(pos..pos + 2, arg);
        } else {
            break;
        }
    }
    s
}

/// Plural category for a count: 0 = one, 1 = few, 2 = many/other.
///
/// The settings views use it for "N files"; Slavic languages need the
/// one/few/many distinction, other bundled locales use singular for 1 and
/// plural otherwise.
pub fn plural_form(lang: &str, count: u64) -> usize {
    let code = LocaleInfo::normalize(lang);
    if matches!(code, "ru" | "uk" | "be") {
        let hundred = count % 100;
        let ten = count % 10;
        if ten == 1 && hundred != 11 {
            0
        } else if (2..=4).contains(&ten) && !(12..=14).contains(&hundred) {
            1
        } else {
            2
        }
    } else if count == 1 {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn collect_keys(val: &serde_json::Value, prefix: &str, out: &mut HashSet<String>) {
        if let serde_json::Value::Object(map) = val {
            for (k, v) in map {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if v.is_object() {
                    collect_keys(v, &p, out);
                } else {
                    out.insert(p);
                }
            }
        }
    }

    #[test]
    fn test_all_locales_have_full_key_parity_with_en() {
        let en_raw = include_str!("../locales/en.json");
        let en_val: serde_json::Value = serde_json::from_str(en_raw).expect("en.json is valid JSON");
        let mut en_keys = HashSet::new();
        collect_keys(&en_val, "", &mut en_keys);

        assert!(!en_keys.is_empty(), "en.json should have keys");

        for loc in SUPPORTED_LOCALES {
            let raw = match loc.code {
                "ar" => include_str!("../locales/ar.json"),
                "cs" => include_str!("../locales/cs.json"),
                "de" => include_str!("../locales/de.json"),
                "en" => include_str!("../locales/en.json"),
                "es" => include_str!("../locales/es.json"),
                "fr" => include_str!("../locales/fr.json"),
                "hi" => include_str!("../locales/hi.json"),
                "it" => include_str!("../locales/it.json"),
                "ja" => include_str!("../locales/ja.json"),
                "ko" => include_str!("../locales/ko.json"),
                "nl" => include_str!("../locales/nl.json"),
                "pl" => include_str!("../locales/pl.json"),
                "pt" => include_str!("../locales/pt.json"),
                "ru" => include_str!("../locales/ru.json"),
                "sv" => include_str!("../locales/sv.json"),
                "tr" => include_str!("../locales/tr.json"),
                "uk" => include_str!("../locales/uk.json"),
                "vi" => include_str!("../locales/vi.json"),
                "zh-CN" => include_str!("../locales/zh-CN.json"),
                "zh-TW" => include_str!("../locales/zh-TW.json"),
                _ => panic!("Unknown locale: {}", loc.code),
            };

            let val: serde_json::Value =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("Failed to parse {}.json: {e}", loc.code));
            let mut loc_keys = HashSet::new();
            collect_keys(&val, "", &mut loc_keys);

            let missing: Vec<_> = en_keys.difference(&loc_keys).collect();
            assert!(
                missing.is_empty(),
                "Locale '{}' is missing {} keys from en.json: {:?}",
                loc.code,
                missing.len(),
                missing
            );
        }
    }

    #[test]
    fn test_i18n_translation_lookup_and_fallback() {
        assert_eq!(t("common.ok", "ru"), "ОК");
        assert_eq!(t("common.ok", "en"), "OK");
        assert_eq!(t("common.ok", "de"), "OK");
        assert_eq!(t("common.ok", "es"), "Aceptar");
        assert_eq!(t("settings.tabs.editor", "pl"), "Edytor");

        // Fallback to en
        assert_eq!(t("common.untitled", "en"), "Untitled");

        // Fallback to key
        assert_eq!(t("non_existing_key.test", "ru"), "non_existing_key.test");

        // Args substitution
        let res = t_fmt("tabs.add_n_tabs", "ru", &["5", "вкладок..."]);
        assert_eq!(res, "Добавить в закладки 5 вкладок...");
    }
}
