use std::collections::HashMap;

use zbus::{
    fdo::{self},
    interface,
};
use zvariant::{ObjectPath, OwnedValue, Value};

use crate::{
    portals::appchooser::{
        desktop_files::{DesktopEntry, find_desktop_entry},
        run_command::{RunCommandError, run_command, run_picker_command},
    },
    terminal::Terminal,
    utils::hashmap::wildcard_get,
};

use super::config::{AppChooserConfig, Command, DefaultMapping, RunnerType};

pub struct AppChooserService {
    pub terminal: Terminal,
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
        let runner_type = &self.config.runner;

        if runner_type.is_none() {
            return Err(fdo::Error::Failed("runner type is unset".into()));
        }

        let runner_type = runner_type.as_ref().unwrap();

        tracing::debug!(
            "ChooseApplication called with handle: {:?}, app_id: {:?}, parent_window: {}, choices: {:?}, options: {:?}",
            handle,
            app_id,
            parent_window,
            choices,
            options
        );

        let uri = options.get("uri");
        let content_type = options.get("content_type");
        let activation_token = options.get("activation_token").map(|s| s.to_string());

        if uri.is_none() || content_type.is_none() {
            tracing::error!("uri or content_type undefined {:?}", &options);
            return Err(fdo::Error::Failed(format!(
                "uri or content_type undefined {:?}",
                &options
            )));
        }

        let uri = uri
            .map(|uri| uri.to_string())
            .map(|uri| uri.trim_matches('"').to_string())
            .map(|uri| uri.trim_start_matches("file://").to_string())
            .unwrap();
        let content_type = content_type.unwrap().to_string();
        let content_type = content_type.trim_matches('"');

        let runner_cmd = match &runner_type {
            RunnerType::Dmenu(cmd) => cmd,
        };

        let new_token =
            activation_token.unwrap_or_else(|| format!("token-{}", rand::random::<u32>()));

        tracing::info!("URI: {}, Content-Type: {}", uri, content_type);

        // TODO: support content_type wildcards

        // if we have a default mapping set for the content type we use that...
        if let Some(option) = wildcard_get(&self.config.defaults, content_type.to_string()) {
            tracing::info!("Selected mapping: {:?}", option);

            let res = match option {
                DefaultMapping::Command(ref cmd) => {
                    let cmd = cmd.with_input_file(uri);
                    Ok(cmd.clone())
                }
                DefaultMapping::CommandChoice(ref cmds) => {
                    let cmds_str: Vec<String> = cmds
                        .clone()
                        .into_iter()
                        .map(|c| c.command.clone())
                        .collect();

                    tracing::info!("{:?}", cmds_str);

                    run_picker_command(runner_cmd, &cmds_str)
                        .await
                        .map(|cmd| {
                            cmds.into_iter()
                                .find(|c| c.command == cmd.trim())
                                .expect("could not find command from options")
                        })
                        .map(|cmd| cmd.with_input_file(uri))
                }
                DefaultMapping::DesktopFile(ref file) => find_desktop_entry(&file)
                    .map(|entry| Ok(entry.command(&self.terminal).with_input_file(uri)))
                    .unwrap_or(Err(RunCommandError::Other(format!(
                        "Could not find desktop entry for {:?}",
                        file
                    )))),
                DefaultMapping::DesktopFileChoice(ref files) => {
                    let desktop_entries: Vec<DesktopEntry> = files
                        .iter()
                        .filter_map(|name| find_desktop_entry(name))
                        .collect();

                    let options = desktop_entries
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect();

                    run_picker_command(runner_cmd, &options)
                        .await
                        .map(|entry| {
                            desktop_entries
                                .iter()
                                .find(|e| e.name == entry.trim())
                                .expect("could not find desktop file from options")
                        })
                        .map(|entry| entry.command(&self.terminal).with_input_file(uri))
                }
            };

            if res.is_err() {
                let err = format!("something went wrong while running {:?}", option);
                tracing::error!("{}", err);
                return Err(fdo::Error::Failed(err));
            }

            let res = res.unwrap();

            let _ = run_command(&res).await?;

            return cmd_ok(&res, &new_token);
        } else {
            tracing::warn!(
                "No default found for {:?}. Defaults: {:?}",
                content_type,
                self.config.defaults
            );
        }

        // no defaults set so lets evaluate user choices
        if choices.is_empty() {
            return Err(fdo::Error::Failed(
                "No application choices provided".to_string(),
            ));
        }

        let desktop_entries: Vec<DesktopEntry> = choices
            .iter()
            .filter_map(|name| find_desktop_entry(name))
            .collect();

        let options = desktop_entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect();

        let res = if desktop_entries.len() == 1 {
            Ok(desktop_entries
                .first()
                .unwrap()
                .command(&self.terminal)
                .with_input_file(uri))
        } else {
            run_picker_command(runner_cmd, &options)
                .await
                .map(|entry| {
                    desktop_entries
                        .iter()
                        .find(|e| e.name == entry.trim())
                        .expect("could not find desktop file from options")
                })
                .map(|entry| entry.command(&self.terminal).with_input_file(uri))
        };

        if res.is_err() {
            let err = format!("something went wrong while running {:?}", options);
            tracing::error!("{}", err);
            return Err(fdo::Error::Failed(err));
        }

        let res = res.unwrap();

        let _ = run_command(&res).await?;

        cmd_ok(&res, &new_token)
    }
}

fn cmd_ok(command: &Command, token: &String) -> fdo::Result<(u32, HashMap<String, OwnedValue>)> {
    let mut m = HashMap::new();

    m.insert(
        "app_id".to_string(),
        zvariant::Str::from(&command.command).into(),
    );

    m.insert(
        "activation_token".to_string(),
        zvariant::Str::from(token).into(),
    );

    Ok((0, m))
}
