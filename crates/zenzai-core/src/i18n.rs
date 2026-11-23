use std::env;

/// Select the UI language once at application startup.
pub fn init() {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "C".into());
    let base = locale.split(['.', '@']).next().unwrap_or(&locale);
    if matches!(base, "C" | "POSIX") {
        rust_i18n::set_locale("en");
        return;
    }

    let languages = env::var("LANGUAGE")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or(locale);
    let available = rust_i18n::available_locales!();

    for language in languages.split(':') {
        if matches!(language, "C" | "POSIX") {
            break;
        }

        let locale = language
            .split(['.', '@'])
            .next()
            .unwrap_or(language)
            .replace('_', "-");
        let base = locale.split('-').next().unwrap_or(&locale);
        if let Some(selected) = available
            .iter()
            .find(|value| value.as_ref() == locale)
            .or_else(|| available.iter().find(|value| value.as_ref() == base))
        {
            rust_i18n::set_locale(selected);
            return;
        }
    }

    rust_i18n::set_locale("en");
}
