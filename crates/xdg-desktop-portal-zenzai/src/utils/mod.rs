pub mod command;

// a trailing `*` matches any suffix, e.g. "image/*" or "org.gnome.*"
pub fn matches_glob(pattern: &str, value: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map_or(pattern == value, |prefix| value.starts_with(prefix))
}
