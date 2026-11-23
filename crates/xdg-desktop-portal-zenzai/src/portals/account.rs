use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use nix::unistd::{Uid, User};
use rust_i18n::t;
use serde::Deserialize;
use tokio::{sync::watch, task};
use url::Url;
use zbus::{Connection, ObjectPath, OwnedVariant, Str, Variant, fdo, interface, message::Header};
use zenzai_core::ipc::AccessRequest;

use crate::utils::access_dialog::prompt_prepared;

#[derive(Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AccountConfig {
    pub enabled: bool,
    pub command: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub image: Option<PathBuf>,
}

// Personal information must not appear in configuration debug logs.
impl fmt::Debug for AccountConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountConfig")
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl Default for AccountConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: None,
            id: None,
            name: None,
            image: None,
        }
    }
}

#[derive(Clone)]
pub struct AccountState {
    pub config: AccountConfig,
    pub command: PathBuf,
}

pub struct AccountService {
    pub state: watch::Receiver<AccountState>,
}

struct Profile {
    id: String,
    name: String,
    image: Option<Url>,
}

fn resolve_profile(config: AccountConfig, user: Option<&User>) -> Result<Profile> {
    let id = config.id.unwrap_or_else(|| {
        user.as_ref()
            .map(|user| user.name.clone())
            .unwrap_or_default()
    });
    let name = config.name.unwrap_or_else(|| {
        user.as_ref()
            .map(|user| {
                user.gecos
                    .to_string_lossy()
                    .split(',')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .unwrap_or_default()
    });
    let image = config
        .image
        .filter(|path| !path.as_os_str().is_empty())
        // An unusable image must not prevent sharing the remaining account information.
        .and_then(|path| {
            resolve_image(&path)
                .inspect_err(|err| tracing::warn!("ignoring account image: {err}"))
                .ok()
        });
    ensure!(
        !id.is_empty() || !name.is_empty() || image.is_some(),
        "no account information available"
    );
    Ok(Profile { id, name, image })
}

fn resolve_image(path: &Path) -> Result<Url> {
    ensure!(
        path.is_absolute(),
        "account image must be an absolute local path"
    );
    ensure!(path.is_file(), "account image is not a regular file");
    Url::from_file_path(path).map_err(|()| anyhow!("invalid account image path"))
}

#[interface(name = "org.freedesktop.impl.portal.Account")]
impl AccountService {
    #[allow(clippy::too_many_arguments)]
    async fn get_user_information(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        window: &str,
        options: HashMap<&str, Variant<'_>>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let reason = options
            .get("reason")
            .map(Variant::downcast_ref::<&str>)
            .transpose()
            .map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?
            .unwrap_or("");
        let AccountState { config, command } = self.state.borrow().clone();
        let prepare = async {
            // NSS lookups may block; keep them off the async executor.
            let profile = task::spawn_blocking(move || {
                let user = if config.id.is_none() || config.name.is_none() {
                    match User::from_uid(Uid::current()) {
                        Ok(user) => user,
                        Err(err) => {
                            tracing::warn!("account lookup failed: {err}");
                            None
                        }
                    }
                } else {
                    None
                };
                resolve_profile(config, user.as_ref())
            })
            .await
            .context("account lookup task failed")??;
            let request = AccessRequest {
                app_id: app_id.into(),
                parent_window: window.into(),
                title: t!("account.share").into(),
                subtitle: t!(
                    "account.requested-by",
                    app = if app_id.is_empty() {
                        t!("account.unknown-application").into_owned()
                    } else {
                        app_id.to_owned()
                    }
                )
                .into(),
                body: format!(
                    "{}: {}\n{}: {}\n{}: {}\n\n{reason}",
                    t!("account.id"),
                    profile.id,
                    t!("account.name"),
                    profile.name,
                    t!("account.image"),
                    profile.image.as_ref().map_or("", Url::as_str)
                ),
                deny_label: Some(t!("common.cancel").into()),
                grant_label: Some(t!("account.share-button").into()),
                icon: Some("avatar-default".into()),
                image: profile
                    .image
                    .as_ref()
                    .and_then(|url| url.to_file_path().ok()),
                modal: true,
                choices: Vec::new(),
            };
            Ok((request, profile))
        };
        match prompt_prepared(command, prepare, handle, connection, header).await? {
            Ok(Some((_, profile))) => {
                let mut results = HashMap::from([
                    ("id".into(), OwnedVariant::from(Str::from(profile.id))),
                    ("name".into(), OwnedVariant::from(Str::from(profile.name))),
                ]);
                if let Some(image) = profile.image {
                    results.insert(
                        "image".into(),
                        OwnedVariant::from(Str::from(image.to_string())),
                    );
                }
                Ok((0, results))
            }
            Ok(None) => Ok((1, HashMap::new())),
            Err(err) => {
                tracing::error!("account dialog failed: {err:#}");
                Ok((2, HashMap::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use nix::unistd::Gid;
    use std::ffi::CString;

    use super::*;

    #[test]
    fn overrides_and_empty_fields_do_not_fall_back() {
        let user = User {
            name: "login".into(),
            passwd: CString::default(),
            uid: Uid::from_raw(1000),
            gid: Gid::from_raw(1000),
            gecos: CString::new("Real Name,Room,Phone").unwrap(),
            dir: PathBuf::from("/home/login"),
            shell: PathBuf::from("/bin/sh"),
        };
        let profile = resolve_profile(AccountConfig::default(), Some(&user)).unwrap();
        assert_eq!(profile.id, "login");
        assert_eq!(profile.name, "Real Name");

        let config = AccountConfig {
            id: Some("public-id".into()),
            name: Some(String::new()),
            ..AccountConfig::default()
        };
        let profile = resolve_profile(config, Some(&user)).unwrap();
        assert_eq!(profile.id, "public-id");
        assert_eq!(profile.name, "");
        assert!(profile.image.is_none());
        assert!(resolve_profile(AccountConfig::default(), None).is_err());

        let config = AccountConfig {
            image: Some("/nonexistent/avatar.png".into()),
            ..AccountConfig::default()
        };
        let profile = resolve_profile(config, Some(&user)).unwrap();
        assert_eq!(profile.id, "login");
        assert!(profile.image.is_none());
    }
}
