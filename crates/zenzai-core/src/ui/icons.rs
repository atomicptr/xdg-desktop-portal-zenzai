use std::path::PathBuf;

#[must_use]
pub fn lookup(name: &str, size: u16, theme: Option<&str>) -> Option<PathBuf> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | "..") {
        return None;
    }

    let lookup = freedesktop_icons::lookup(name).with_size(size);
    let lookup = if let Some(theme) = theme {
        lookup.with_theme(theme)
    } else {
        lookup
    };
    lookup.find()
}
