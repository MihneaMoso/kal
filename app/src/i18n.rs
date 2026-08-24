//! i18n scaffolding (spec §6): Fluent-based string lookup.
//!
//! Usage: `{i18n::tr("save")}` in RSX or code. Locale selection comes from
//! settings later; today only en-US ships, which is also the fallback.

use std::collections::HashMap;
use std::sync::OnceLock;

const EN_US: &str = include_str!("../i18n/en-US/main.ftl");

/// Minimal Fluent-style parser for our flat `key = value` files, avoiding the
/// heavier runtime until more locales land. Values may not span lines yet.
fn parse_ftl(text: &str) -> HashMap<&'static str, String> {
    let mut map = HashMap::new();
    let owned = Box::leak(text.to_string().into_boxed_str());
    for line in owned.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(" = ") {
            map.insert(key.trim(), value.trim().to_string());
        }
    }
    map
}

fn strings() -> &'static HashMap<&'static str, String> {
    static BUNDLE: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    BUNDLE.get_or_init(|| parse_ftl(EN_US))
}

/// Look up a localized string by message id; falls back to the key itself so
/// missing translations are visible instead of crashing.
pub fn tr(key: &str) -> String {
    strings()
        .get(key)
        .cloned()
        .unwrap_or_else(|| format!("<{key}>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_resolve() {
        assert_eq!(tr("app-title"), "Kal");
        assert_eq!(tr("view-agenda"), "Agenda");
        assert!(tr("editor-new-birthday").starts_with("New "));
    }

    #[test]
    fn unknown_key_is_visible_fallback() {
        assert_eq!(tr("definitely-not-a-key"), "<definitely-not-a-key>");
    }

    #[test]
    fn comments_and_blanks_skipped() {
        // '# Kal' header comment must not become a message.
        assert!(tr("# Kal").starts_with('<'));
    }
}
