use std::{collections::HashMap, sync::Arc};

use anyhow::{Error, Result};
use futures_util::StreamExt;
use tempfile::NamedTempFile;
use tokio::sync::{Mutex, OnceCell};
use zbus::{
    Connection, Message, OwnedVariant, Proxy, Str, fdo, interface, object_server::SignalEmitter,
};

use crate::constants::DBUS_PATH;

use super::daemon::{Action, DaemonProxy, Dict, NAME, Notification};

type Key = (String, String);

struct Record {
    key: Key,
    actions: HashMap<String, Action>,
    token: Option<String>,
    _icon: Option<NamedTempFile>,
}

#[derive(Default)]
struct State {
    owner: String,
    markup: bool,
    ids: HashMap<Key, u32>,
    records: HashMap<u32, Record>,
}

impl State {
    fn clear(&mut self) {
        self.ids.clear();
        self.records.clear();
        self.owner.clear();
    }

    fn remove(&mut self, id: u32) {
        if let Some(record) = self.records.remove(&id) {
            self.ids.remove(&record.key);
        }
    }

    async fn daemon(&mut self, connection: &Connection) -> Result<DaemonProxy<'static>> {
        let bus = fdo::DBusProxy::new(connection).await?;
        let name = NAME.try_into()?;

        let owner = match bus.get_name_owner(name).await {
            Ok(owner) => owner,
            Err(fdo::Error::NameHasNoOwner(_)) => {
                bus.start_service_by_name(NAME.try_into()?, 0).await?;
                bus.get_name_owner(NAME.try_into()?).await?
            }

            Err(err) => return Err(err.into()),
        };

        let daemon = DaemonProxy::builder(connection)
            .destination(owner.clone())
            .build()
            .await?;

        if self.owner != owner.as_str() {
            self.clear();
            self.markup = daemon
                .get_capabilities()
                .await?
                .iter()
                .any(|cap| cap == "body-markup");
            self.owner = owner.to_string();
        }

        Ok(daemon)
    }
}

#[derive(Default)]
pub struct NotificationService {
    state: Arc<Mutex<State>>,
    listening: OnceCell<()>,
}

impl NotificationService {
    async fn listen(&self, connection: &Connection) -> Result<()> {
        self.listening
            .get_or_try_init(|| async {
                let daemon = DaemonProxy::new(connection).await?;
                let mut signals = daemon.inner().receive_all_signals().await?;
                let bus = fdo::DBusProxy::new(connection).await?;
                let mut owners = bus.receive_name_owner_changed_with_args(&[(0, NAME)]).await?;
                let state = Arc::clone(&self.state);
                let connection = connection.clone();

                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(message) = signals.next() => {
                                if let Err(err) = handle_signal(&connection, &state, message).await {
                                    tracing::warn!("notification action failed: {err:#}");
                                }
                            }

                            Some(event) = owners.next() => {
                                if let Ok(args) = event.args() {
                                    let mut state = state.lock().await;

                                    if args.old_owner().as_ref()
                                        .is_some_and(|owner| owner.as_str() == state.owner)
                                    {
                                        state.clear();
                                    }
                                }
                            }

                            else => break,
                        }
                    }
                });

                Ok::<_, Error>(())
            })
            .await?;

        Ok(())
    }
}

#[interface(name = "org.freedesktop.impl.portal.Notification")]
impl NotificationService {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    async fn add_notification(
        &self,
        app_id: &str,
        id: &str,
        notification: Dict,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let result: Result<()> = async {
            self.listen(connection).await?;

            // Serialize requests and signals so a fast daemon cannot close or invoke
            // a notification before its returned ID has been recorded.
            let mut state = self.state.lock().await;
            let daemon = state.daemon(connection).await?;
            let notification = Notification::parse(app_id, notification, state.markup).await?;
            let key = (app_id.to_owned(), id.to_owned());
            let replaces = state.ids.get(&key).copied().unwrap_or_default();
            let daemon_id = daemon
                .notify(
                    "",
                    replaces,
                    &notification.icon_name,
                    &notification.title,
                    &notification.body,
                    &notification.actions,
                    &notification.hints,
                    -1,
                )
                .await?;

            state.remove(replaces);
            state.ids.insert(key.clone(), daemon_id);
            state.records.insert(
                daemon_id,
                Record {
                    key,
                    actions: notification.action_map,
                    token: None,
                    _icon: notification.icon_file,
                },
            );

            Ok(())
        }
        .await;

        result.map_err(|err| fdo::Error::Failed(format!("notification: {err:#}")))
    }

    async fn remove_notification(
        &self,
        app_id: &str,
        id: &str,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let mut state = self.state.lock().await;
        let key = (app_id.to_owned(), id.to_owned());

        if !state.ids.contains_key(&key) {
            return Ok(());
        }

        let daemon = state
            .daemon(connection)
            .await
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        if let Some(id) = state.ids.get(&key).copied() {
            match daemon.close_notification(id).await {
                // The notification may have expired before withdrawal arrived.
                Ok(()) | Err(fdo::Error::InvalidArgs(_)) => state.remove(id),
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    #[zbus(signal)]
    pub async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        app_id: &str,
        id: &str,
        action: &str,
        parameter: Vec<OwnedVariant>,
    ) -> zbus::Result<()>;
}

async fn handle_signal(
    connection: &Connection,
    state: &Mutex<State>,
    message: Message,
) -> Result<()> {
    let mut state = state.lock().await;
    let header = message.header();

    if header
        .sender()
        .is_none_or(|sender| sender.as_str() != state.owner)
    {
        return Ok(());
    }

    match header.member().map(|member| member.as_str()) {
        Some("NotificationClosed") => {
            let (id, _reason): (u32, u32) = message.body().deserialize()?;
            state.remove(id);
        }

        Some("ActivationToken") => {
            let (id, token): (u32, String) = message.body().deserialize()?;

            if let Some(record) = state.records.get_mut(&id) {
                record.token = Some(token);
            }
        }

        Some("ActionInvoked") => {
            let (id, key): (u32, String) = message.body().deserialize()?;

            let Some(record) = state.records.get_mut(&id) else {
                return Ok(());
            };

            let Some(action) = record.actions.get(&key) else {
                return Ok(());
            };

            let action = Action {
                name: action.name.clone(),
                target: action
                    .target
                    .as_ref()
                    .map(OwnedVariant::try_clone)
                    .transpose()?,
            };
            let key = record.key.clone();
            let token = record.token.take();
            drop(state);
            let connection = connection.clone();

            tokio::spawn(async move {
                if let Err(err) = activate(connection, key, action, token).await {
                    tracing::warn!("notification activation failed: {err:#}");
                }
            });
        }

        _ => {}
    }

    Ok(())
}

async fn activate(
    connection: Connection,
    key: Key,
    action: Action,
    token: Option<String>,
) -> Result<()> {
    let (app_id, id) = key;
    let path = format!("/{}", app_id.replace('.', "/").replace('-', "_"));
    let mut platform = Dict::new();

    if let Some(token) = token {
        platform.insert(
            "desktop-startup-id".to_owned(),
            Str::from(token.as_str()).into(),
        );
        platform.insert("activation-token".to_owned(), Str::from(token).into());
    }

    let parameters: Vec<_> = action.target.into_iter().collect();
    let app = Proxy::new(
        &connection,
        app_id.as_str(),
        path.as_str(),
        "org.freedesktop.Application",
    )
    .await;

    if let Some(name) = action.name.strip_prefix("app.") {
        app?.call::<_, _, ()>("ActivateAction", &(name, parameters, platform))
            .await?;
    } else {
        // Host applications may have no app ID. Activation is best-effort and
        // must not prevent the action signal or wait for an application reply.
        if let Ok(app) = app
            && let Err(err) = app.call_noreply("Activate", &(platform,)).await
        {
            tracing::debug!("could not activate notification application {app_id}: {err}");
        }

        let emitter = SignalEmitter::new(&connection, DBUS_PATH)?;
        NotificationService::action_invoked(&emitter, &app_id, &id, &action.name, parameters)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_is_scoped_to_application_and_daemon_generation() {
        let mut state = State::default();

        for (app, daemon_id) in [("org.example.One", 1), ("org.example.Two", 2)] {
            let key = (app.to_owned(), "same-id".to_owned());
            state.ids.insert(key.clone(), daemon_id);
            state.records.insert(
                daemon_id,
                Record {
                    key,
                    actions: HashMap::new(),
                    token: None,
                    _icon: None,
                },
            );
        }

        state.remove(1);
        assert_eq!(state.ids.len(), 1);
        assert_eq!(
            state.ids[&("org.example.Two".to_owned(), "same-id".to_owned())],
            2
        );

        state.clear();
        assert!(state.ids.is_empty());
        assert!(state.records.is_empty());
    }
}
