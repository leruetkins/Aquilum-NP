use std::collections::HashSet;

/// Returns the string literals on a source line, ignoring comments.
fn string_literals(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => break,
            b'"' => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'"' {
                    if bytes[end] == b'\\' {
                        end += 1;
                    }
                    end += 1;
                }
                out.push(&line[start..end.min(line.len())]);
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn has_cyrillic_word(literal: &str) -> bool {
    literal.chars().count() > 1 && literal.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

fn menu_sources() -> [(&'static str, &'static str); 3] {
    [
        ("editor_context_menu.rs", include_str!("editor_context_menu.rs")),
        ("more_options_menu.rs", include_str!("more_options_menu.rs")),
        ("ribbon_menu.rs", include_str!("ribbon_menu.rs")),
    ]
}

#[test]
fn context_menus_have_no_hardcoded_ui_text() {
    for (name, src) in menu_sources() {
        let body = src.split("#[cfg(test)]").next().unwrap_or(src);
        let mut allow_next_line = false;
        for (index, line) in body.lines().enumerate() {
            if allow_next_line {
                allow_next_line = false;
                continue;
            }
            if line.contains("i18n-allow") {
                allow_next_line = true;
                continue;
            }
            if line.trim_start().starts_with("//") {
                continue;
            }
            for literal in string_literals(line) {
                assert!(
                    !has_cyrillic_word(literal),
                    "{name}:{} has hardcoded UI text: {literal:?}",
                    index + 1
                );
            }
        }
    }
}

#[test]
fn context_menus_have_no_russian_only_switch() {
    for (name, src) in menu_sources() {
        let body = src.split("#[cfg(test)]").next().unwrap_or(src);
        for (index, line) in body.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            let switch = line.contains("starts_with(\"ru\")") || line.contains("is_ru");
            assert!(
                !switch,
                "{name}:{} must not branch on the Russian locale",
                index + 1
            );
        }
    }
}

#[test]
fn menu_locale_keys_exist() {
    let prefixes = ["editor.context.", "app.more_options.", "app.ribbon."];
    let mut used = HashSet::new();

    for (_, src) in menu_sources() {
        let body = src.split("#[cfg(test)]").next().unwrap_or(src);
        for line in body.lines() {
            for literal in string_literals(line) {
                if prefixes.iter().any(|prefix| literal.starts_with(prefix)) {
                    used.insert(literal);
                }
            }
        }
    }

    for key in used {
        let translated = crate::i18n::t(key, "en");
        assert_ne!(translated, key, "missing locale key: {key}");
    }
}
