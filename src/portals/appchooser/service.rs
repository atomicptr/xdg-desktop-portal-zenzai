use std::collections::HashMap;

use zbus::{
    fdo::{self},
    interface,
};
use zvariant::{ObjectPath, OwnedValue, Value};

use super::config::{AppChooserConfig, RunnerType};

pub struct AppChooserService {
    pub config: AppChooserConfig,
}

#[interface(name = "org.freedesktop.impl.portal.AppChooser")]
impl AppChooserService {
    async fn choose_application(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        choices: Vec<&str>,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        tracing::info!(
            "ChooseApplication called with handle: {:?}, app_id: {:?}, parent_window: {}, choices: {:?}, options: {:?}",
            handle,
            app_id,
            parent_window,
            choices,
            options
        );
        tracing::info!("Config set as {:?}", self.config);

        if let Some(runner) = &self.config.runner_type {
            match runner {
                RunnerType::Dmenu(cmd) => {
                    tracing::info!("run: {:?} {:?}", cmd.command, cmd.arguments)
                }
            }
        }

        Err(fdo::Error::Failed("not yet implemented".to_string()))
    }

    async fn update_choices(&self, handle: ObjectPath<'_>, choices: Vec<&str>) -> fdo::Result<()> {
        println!(
            "UpdateChoices called with handle: {}, choices: {:?}",
            handle, choices
        );
        Ok(())
    }
}
