//! Minimal runtime translation layer.
//!
//! Loads the active language from `/mnt/SDCARD/.userdata/shared/minuisettings.txt`
//! (the same `language=<code>` line written by NextUI's own i18n system, so
//! Updater follows whatever the user picked in Settings → System → Language)
//! and resolves keys against a `HashMap` parsed from a bundled `.lang` file
//! embedded at compile time via `include_str!`.
//!
//! English is the fallback: if a key has no entry in the active language
//! table, the English table is consulted; if still missing, the key itself
//! is returned untouched, so unrecognised keys remain readable.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::SDCARD_ROOT;

const EN_LANG: &str = include_str!("../lang/en.lang");
const FR_LANG: &str = include_str!("../lang/fr.lang");

struct Tables {
    active: HashMap<String, String>,
    english: HashMap<String, String>,
    code: String,
}

static TABLES: OnceLock<RwLock<Tables>> = OnceLock::new();

fn parse(src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim_end_matches(['\r', '\n', ' ', '\t']);
        if key.is_empty() {
            continue;
        }
        // Unescape \n produced by the .lang format.
        let value = value.replace("\\n", "\n");
        out.insert(key.to_owned(), value);
    }
    out
}

fn detect_language() -> String {
    let path = format!("{SDCARD_ROOT}.userdata/shared/minuisettings.txt");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return "en".to_owned();
    };
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("language=") {
            let code = rest.trim();
            if !code.is_empty() {
                return code.to_owned();
            }
        }
    }
    "en".to_owned()
}

fn build(code: &str) -> Tables {
    let english = parse(EN_LANG);
    let active = if code == "en" {
        english.clone()
    } else if code == "fr" {
        parse(FR_LANG)
    } else {
        english.clone()
    };
    Tables {
        active,
        english,
        code: code.to_owned(),
    }
}

fn tables() -> &'static RwLock<Tables> {
    TABLES.get_or_init(|| RwLock::new(build(&detect_language())))
}

/// Translate a key against the active language, falling back to English then
/// to the key string itself.
pub fn t(key: &str) -> String {
    let guard = tables().read();
    if let Some(v) = guard.active.get(key) {
        return v.clone();
    }
    if let Some(v) = guard.english.get(key) {
        return v.clone();
    }
    key.to_owned()
}

/// Active language code (e.g. "en", "fr"). Useful for debugging.
#[allow(dead_code)]
pub fn active_code() -> String {
    tables().read().code.clone()
}

/// Reload from disk; e.g. after the user changed language in Settings.
#[allow(dead_code)]
pub fn reload() {
    let new = build(&detect_language());
    *tables().write() = new;
}

/// Convenience macro mirroring the C-side `T("key")` API used by NextUI core.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::t($key)
    };
}
