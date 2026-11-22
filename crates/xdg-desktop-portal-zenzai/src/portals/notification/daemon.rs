use std::{collections::HashMap, io::Write};

use anyhow::{Result, bail};
use tempfile::NamedTempFile;
use tokio::task;
use zbus::{OwnedVariant, Str, Variant, fdo, proxy};

pub const NAME: &str = "org.freedesktop.Notifications";
const MAX_ICON_BYTES: usize = 4 * 1024 * 1024;

pub type Dict = HashMap<String, OwnedVariant>;

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
pub trait Daemon {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[String],
        hints: &Dict,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> fdo::Result<()>;

    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;
}

pub struct Action {
    pub name: String,
    pub target: Option<OwnedVariant>,
}

pub struct Notification {
    pub title: String,
    pub body: String,
    pub icon_name: String,
    pub icon_file: Option<NamedTempFile>,
    pub actions: Vec<String>,
    pub action_map: HashMap<String, Action>,
    pub hints: Dict,
}

impl Notification {
    pub async fn parse(app_id: &str, mut data: Dict, markup: bool) -> Result<Self> {
        let title = string(&data, "title").to_owned();
        let body = string(&data, "body");
        let body = if markup {
            body.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        } else {
            body.to_owned()
        };

        let urgency: u8 = match string(&data, "priority") {
            "low" => 0,
            "urgent" => 2,
            _ => 1,
        };

        let mut hints = Dict::from([
            ("desktop-entry".to_owned(), Str::from(app_id).into()),
            ("urgency".to_owned(), urgency.into()),
        ]);
        let mut actions = Vec::new();
        let mut action_map = HashMap::new();

        if let Some(name) = data.remove("default-action") {
            action_map.insert(
                "default".to_owned(),
                Action {
                    name: String::try_from(name)?,
                    target: data.remove("default-action-target"),
                },
            );

            actions.extend(["default".to_owned(), String::new()]);
        }

        if let Some(buttons) = data.remove("buttons") {
            for (index, mut button) in Vec::<Dict>::try_from(buttons)?.into_iter().enumerate() {
                // Opaque keys avoid collisions with "default" and preserve arbitrary targets.
                let key = format!("button-{index}");
                let label = string(&button, "label").to_owned();
                let name = string(&button, "action").to_owned();

                if name.is_empty() {
                    continue;
                }

                actions.extend([key.clone(), label]);
                action_map.insert(
                    key,
                    Action {
                        name,
                        target: button.remove("target"),
                    },
                );
            }
        }

        let mut icon_name = String::new();
        let mut icon_file = None;

        if let Some(icon) = data.remove("icon") {
            if let Ok(name) = <&str>::try_from(&icon) {
                icon_name = name.to_owned();
            } else {
                let (kind, value) = <(String, OwnedVariant)>::try_from(Variant::from(icon))?;

                match kind.as_str() {
                    "themed" => {
                        icon_name = Variant::from(value)
                            .downcast::<Vec<String>>()?
                            .into_iter()
                            .next()
                            .unwrap_or_default();
                    }

                    "bytes" => {
                        let bytes = Variant::from(value).downcast::<Vec<u8>>()?;

                        if bytes.len() > MAX_ICON_BYTES {
                            bail!("notification icon exceeds {MAX_ICON_BYTES} bytes");
                        }

                        let file = task::spawn_blocking(move || -> Result<_> {
                            let mut file = NamedTempFile::new()?;

                            file.write_all(&bytes)?;

                            Ok(file)
                        })
                        .await??;

                        hints.insert(
                            "image-path".to_owned(),
                            Str::from(file.path().to_string_lossy().into_owned()).into(),
                        );
                        icon_file = Some(file);
                    }

                    _ => bail!("unsupported notification icon type: {kind}"),
                }
            }
        }

        Ok(Self {
            title,
            body,
            icon_name,
            icon_file,
            actions,
            action_map,
            hints,
        })
    }
}

fn string<'a>(dict: &'a Dict, key: &str) -> &'a str {
    dict.get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn translates_actions_without_collisions_and_preserves_targets() {
        let buttons = vec![Dict::from([
            ("label".to_owned(), Str::from("Open").into()),
            ("action".to_owned(), Str::from("default").into()),
            ("target".to_owned(), 42u32.into()),
        ])];
        let data = Dict::from([
            ("body".to_owned(), Str::from("<b>A & B</b>").into()),
            ("priority".to_owned(), Str::from("high").into()),
            ("default-action".to_owned(), Str::from("app.open").into()),
            (
                "default-action-target".to_owned(),
                Str::from("document").into(),
            ),
            (
                "buttons".to_owned(),
                Variant::from(buttons).try_into().unwrap(),
            ),
            (
                "icon".to_owned(),
                Variant::from(("themed", Variant::from(vec!["mail-unread".to_owned()])))
                    .try_into()
                    .unwrap(),
            ),
        ]);

        let notification = Notification::parse("org.example.App", data, true)
            .await
            .unwrap();

        assert_eq!(notification.icon_name, "mail-unread");
        assert_eq!(notification.body, "&lt;b&gt;A &amp; B&lt;/b&gt;");
        assert_eq!(u8::try_from(&notification.hints["urgency"]).unwrap(), 1);
        assert_eq!(notification.actions, ["default", "", "button-0", "Open"]);
        assert_eq!(notification.action_map["default"].name, "app.open");
        assert_eq!(
            <&str>::try_from(notification.action_map["default"].target.as_ref().unwrap()).unwrap(),
            "document"
        );

        assert_eq!(notification.action_map["button-0"].name, "default");
        assert_eq!(
            u32::try_from(notification.action_map["button-0"].target.as_ref().unwrap()).unwrap(),
            42
        );
    }
}
