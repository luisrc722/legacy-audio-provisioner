use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Es,
    En,
}

static ACTIVE_LOCALE: OnceLock<Locale> = OnceLock::new();

pub fn init_locale(cli_lang: Option<&str>) {
    let locale = cli_lang
        .and_then(parse_locale)
        .or_else(|| std::env::var("LAP_LANG").ok().as_deref().and_then(parse_locale))
        .unwrap_or(Locale::Es);

    let _ = ACTIVE_LOCALE.set(locale);
}

pub fn tr<'a>(es: &'a str, en: &'a str) -> &'a str {
    match ACTIVE_LOCALE.get().copied().unwrap_or(Locale::Es) {
        Locale::Es => es,
        Locale::En => en,
    }
}

fn parse_locale(raw: &str) -> Option<Locale> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "es" | "es-es" | "es_mx" | "es-mx" => Some(Locale::Es),
        "en" | "en-us" | "en_us" | "en-gb" | "en_gb" => Some(Locale::En),
        _ => None,
    }
}
