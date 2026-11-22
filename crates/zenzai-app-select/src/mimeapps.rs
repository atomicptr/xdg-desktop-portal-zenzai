use std::{
    collections::HashSet,
    env,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Seek, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, ensure};
use ini::{EscapePolicy, Ini, ParseOption};
use rustix::fs::OFlags;

const MIMEAPPS: &str = "mimeapps.list";
const DEFAULTS: &str = "Default Applications";
const ADDED: &str = "Added Associations";
const REMOVED: &str = "Removed Associations";

pub fn default_app(content_type: &str) -> Result<Option<String>> {
    if !has_association_file() {
        return Ok(None);
    }

    let output = Command::new("xdg-mime")
        .args(["query", "default", content_type])
        .output()
        .context("could not run xdg-mime query default")?;
    ensure!(
        output.status.success(),
        "MIME default lookup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let id = String::from_utf8(output.stdout)?.trim().to_string();
    Ok((!id.is_empty()).then_some(id))
}

fn has_association_file() -> bool {
    let desktops: Vec<_> = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .map(|name| format!("{}-{MIMEAPPS}", name.to_ascii_lowercase()))
        .collect();
    let system_dirs = |key: &str, fallback: &str| {
        env::split_paths(
            &env::var_os(key)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| fallback.into()),
        )
        .filter(|path| path.is_absolute())
        .collect::<Vec<_>>()
    };
    let config = dirs::config_dir()
        .into_iter()
        .chain(system_dirs("XDG_CONFIG_DIRS", "/etc/xdg"));
    let data = dirs::data_dir()
        .into_iter()
        .chain(system_dirs("XDG_DATA_DIRS", "/usr/local/share:/usr/share"))
        .map(|path| path.join("applications"));

    config.chain(data).any(|root| {
        root.join(MIMEAPPS).is_file() || desktops.iter().any(|name| root.join(name).is_file())
    })
}

pub fn remember(content_type: &str, id: &str) -> Result<()> {
    let Some(directory) = dirs::config_dir() else {
        return Ok(());
    };
    if !update_file(&directory.join(MIMEAPPS), content_type, id)? {
        return Ok(());
    }

    match default_app(content_type) {
        Ok(Some(default)) if default != id => {
            tracing::warn!(
                "saved MIME association for {id}, but the effective default is {default}"
            );
        }
        Err(err) => tracing::warn!("could not verify saved MIME association: {err:#}"),
        _ => {}
    }
    Ok(())
}

fn update_file(path: &Path, content_type: &str, id: &str) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    ensure!(
        metadata.is_file(),
        "{} is not a regular file (managed symlinks are not modified)",
        path.display()
    );
    ensure!(
        !metadata.permissions().readonly(),
        "{} is read-only",
        path.display()
    );

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    file.lock()?;
    let metadata = file.metadata()?;
    let mut original = String::new();
    file.read_to_string(&mut original)?;
    let updated = update(&original, content_type, id)?;
    if updated == original {
        return Ok(true);
    }

    let current = fs::symlink_metadata(path)?;
    ensure!(
        current.is_file()
            && current.dev() == metadata.dev()
            && current.ino() == metadata.ino()
            && fs::read_to_string(path)? == original,
        "MIME associations changed while saving; leaving them untouched"
    );
    let directory = path.parent().context("MIME file has no parent directory")?;
    write_with_backup(&mut file, directory, &original, &updated)?;
    Ok(true)
}

// Descriptor writes preserve concurrent replacements; the backup covers partial writes.
fn write_with_backup(
    file: &mut File,
    directory: &Path,
    original: &str,
    updated: &str,
) -> Result<()> {
    let mut backup = tempfile::Builder::new()
        .prefix(".mimeapps-backup-")
        .tempfile_in(directory)?;
    backup.write_all(original.as_bytes())?;
    backup.as_file().sync_all()?;
    let (_backup, backup_path) = backup.keep()?;

    let result = (|| {
        file.rewind()?;
        file.write_all(updated.as_bytes())?;
        file.set_len(updated.len() as u64)?;
        file.sync_all()
    })();
    result.with_context(|| {
        format!(
            "MIME save failed; original contents retained at {}",
            backup_path.display()
        )
    })?;
    fs::remove_file(backup_path)?;
    Ok(())
}

fn update(contents: &str, content_type: &str, id: &str) -> Result<String> {
    ensure!(
        content_type.split('/').count() == 2
            && content_type.split('/').all(|part| !part.is_empty())
            && content_type.starts_with(|ch: char| ch.is_ascii_alphanumeric())
            && content_type
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&byte)),
        "invalid MIME type"
    );
    ensure!(
        id.ends_with(".desktop")
            && id.len() > ".desktop".len()
            && !id.contains(['/', '\\', ';', '=', '[', ']'])
            && !id.chars().any(char::is_whitespace),
        "invalid desktop file ID"
    );

    // rust-ini consumes backslash-newline even with escapes disabled.
    let mut ini = Ini::load_from_str_opt(
        &contents
            .trim_start_matches('\u{feff}')
            .replace('\\', "\\\\"),
        ParseOption {
            enabled_quote: false,
            ..Default::default()
        },
    )?;
    ensure!(
        ini.general_section().is_empty(),
        "MIME entries outside a section"
    );

    let mut sections = HashSet::new();
    for (name, properties) in &ini {
        if let Some(name) = name {
            ensure!(
                !name.is_empty() && !name.contains(['[', ']', '\n', '\r']) && sections.insert(name),
                "invalid or duplicate MIME section"
            );
        }

        let mut keys = HashSet::new();
        for (key, value) in properties {
            ensure!(
                !key.is_empty()
                    && !key.contains(['\n', '\r'])
                    && keys.insert(key)
                    && !value.contains(['\n', '\r']),
                "invalid or duplicate MIME association"
            );
        }
    }

    for name in [DEFAULTS, ADDED, REMOVED] {
        let previous = ini.get_from(Some(name), content_type).unwrap_or_default();
        let ids: Vec<_> = (name != REMOVED)
            .then_some(id)
            .into_iter()
            .chain(
                previous
                    .split(';')
                    .filter(|value| !value.is_empty() && *value != id),
            )
            .collect();
        if ids.is_empty() {
            ini.delete_from(Some(name), content_type);
        } else {
            let value = format!("{};", ids.join(";"));
            ini.with_section(Some(name)).set(content_type, value);
        }
    }

    let mut updated = Vec::new();
    ini.write_to_policy(&mut updated, EscapePolicy::Nothing)?;
    Ok(String::from_utf8(updated)?)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    const MIME: &str = "text/plain";
    const APP: &str = "chosen.desktop";

    #[test]
    fn updating_normalizes_formatting_and_reconciles_associations() {
        let original = concat!(
            "\u{feff}# discarded comment\r\n[ Default Applications ]\r\n",
            "text/plain = old.desktop;chosen.desktop;fallback.desktop;#literal.desktop;\r\n",
            "image/png=viewer.desktop;\r\n[ Added Associations\t]\r\n",
            "text/plain=old.desktop;\r\n[ Removed Associations ]\r\n",
            "text/plain=chosen.desktop;blocked.desktop;\r\n[Other]\r\n",
            "custom = \"literal\\svalue\" # literal;\r\ntrailing=literal\\\n"
        );
        let updated = update(original, MIME, APP).unwrap();
        assert_eq!(
            updated,
            concat!(
                "[Default Applications]\nimage/png=viewer.desktop;\n",
                "text/plain=chosen.desktop;old.desktop;fallback.desktop;#literal.desktop;\n",
                "\n[Added Associations]\n",
                "text/plain=chosen.desktop;old.desktop;\n\n[Removed Associations]\n",
                "text/plain=blocked.desktop;\n\n[Other]\n",
                "custom=\"literal\\svalue\" # literal;\ntrailing=literal\\\n"
            )
        );
        assert_eq!(update(&updated, MIME, APP).unwrap(), updated);

        let updated = update(
            "[Removed Associations]\ntext/plain=chosen.desktop;",
            MIME,
            APP,
        )
        .unwrap();
        assert!(!updated.contains("text/plain=;"));
        assert_eq!(updated.matches("text/plain=").count(), 2);
    }

    #[test]
    fn updating_adds_missing_groups_and_rejects_ambiguous_input() {
        for original in ["", "# discarded", "[Other]\nkey=value"] {
            let updated = update(original, MIME, APP).unwrap();
            assert!(updated.contains("[Default Applications]\ntext/plain=chosen.desktop;\n"));
            assert!(updated.contains("[Added Associations]\ntext/plain=chosen.desktop;\n"));
            assert!(!updated.contains("[Removed Associations]"));
            assert_eq!(update(&updated, MIME, APP).unwrap(), updated);
        }

        for original in [
            "not an ini file",
            "[Default Applications]\n[ Default Applications ]\n",
            "[Default Applications]\ntext/plain=a.desktop;\ntext/plain=b.desktop;\n",
            "[Default Applications]\n[broken=value",
            "text/plain=outside.desktop;\n",
            "[Default Applications]\ntext/plain=old.desktop;\\\nother.desktop;\n",
        ] {
            assert!(update(original, MIME, APP).is_err(), "{original:?}");
        }
        assert!(update("", "text/plain\ninjected=value", APP).is_err());
        assert!(update("", "#text/plain", APP).is_err());
        assert!(update("", MIME, "a.desktop;injected.desktop").is_err());
    }

    #[test]
    fn saving_only_updates_existing_writable_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MIMEAPPS);
        assert!(!update_file(&path, MIME, APP).unwrap());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        assert!(!update_file(&directory.path().join("missing").join(MIMEAPPS), MIME, APP).unwrap());
        assert!(!directory.path().join("missing").exists());

        let original = "# preserved\n[Default Applications]\ntext/plain=old.desktop;\n";
        fs::write(&path, original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(update_file(&path, MIME, APP).unwrap());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            update(original, MIME, APP).unwrap()
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

        fs::write(&path, "malformed").unwrap();
        assert!(update_file(&path, MIME, APP).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "malformed");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o440)).unwrap();
        assert!(update_file(&path, MIME, APP).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "malformed");

        let link = directory.path().join("managed.list");
        symlink(&path, &link).unwrap();
        assert!(update_file(&link, MIME, APP).is_err());
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&path).unwrap(), "malformed");
    }

    #[test]
    fn commit_never_replaces_a_concurrently_changed_destination() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MIMEAPPS);
        let managed = directory.path().join("managed.list");
        fs::write(&managed, "managed contents").unwrap();

        for replacement in [None, Some(false), Some(true)] {
            fs::write(&path, "original").unwrap();
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            file.lock().unwrap();
            fs::remove_file(&path).unwrap();
            match replacement {
                None => {}
                Some(false) => fs::write(&path, "newer contents").unwrap(),
                Some(true) => symlink(&managed, &path).unwrap(),
            }
            write_with_backup(&mut file, directory.path(), "original", "updated").unwrap();
            match replacement {
                None => assert!(!path.exists()),
                Some(false) => assert_eq!(fs::read_to_string(&path).unwrap(), "newer contents"),
                Some(true) => {
                    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
                    assert_eq!(fs::read_to_string(&managed).unwrap(), "managed contents");
                }
            }
            if replacement.is_some() {
                fs::remove_file(&path).unwrap();
            }
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_commit_keeps_a_recovery_copy() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MIMEAPPS);
        fs::write(&path, "original").unwrap();
        let mut file = File::open(&path).unwrap();
        let err =
            write_with_backup(&mut file, directory.path(), "original", "updated").unwrap_err();
        assert!(err.to_string().contains("original contents retained at"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        let backup = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".mimeapps-backup-")
            })
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "original");
    }
}
