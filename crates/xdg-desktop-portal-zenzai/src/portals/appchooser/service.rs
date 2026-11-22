use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use termlauncher::Terminal;
use url::Url;
use zbus::{ObjectPath, OwnedVariant, Variant, fdo, interface};

use crate::utils::matches_glob;

use super::{
    config::{AppChooserConfig, RunnerType, Target},
    launch::{Launchable, pick, resolve, spawn},
};

const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;

pub struct AppChooserService {
    pub terminal: Terminal,
    pub config: AppChooserConfig,
}

impl AppChooserService {
    // configured defaults win, the most specific pattern first, then the application's suggestions
    fn targets(&self, content_type: &str, choices: &[&str]) -> Vec<Target> {
        let defaults = &self.config.defaults;

        let mapping = defaults.get(content_type).or_else(|| {
            defaults
                .iter()
                .filter(|(pattern, _)| matches_glob(pattern, content_type))
                .max_by_key(|(pattern, _)| pattern.len())
                .map(|(_, mapping)| mapping)
        });

        match mapping {
            Some(mapping) => mapping.targets().to_vec(),
            None => choices
                .iter()
                .map(|choice| Target::DesktopFile(choice.to_string()))
                .collect(),
        }
    }

    async fn choose(&self, file: &str, targets: &[Target]) -> Result<u32> {
        // a single choice needs no picker, unresolvable desktop files are skipped like before
        let launchables: Vec<Launchable> = targets
            .iter()
            .filter_map(|target| {
                resolve(target, file, &self.terminal)
                    .inspect_err(|err| tracing::warn!("{err:#}"))
                    .ok()
            })
            .collect();

        if launchables.is_empty() {
            bail!("no application available for {file}");
        }

        let runner = match &self.config.runner {
            Some(RunnerType::Dmenu(runner)) => runner,
            None if launchables.len() == 1 => &launchables[0].command,
            None => bail!("several applications match but no runner is configured"),
        };

        match pick(runner, &launchables).await? {
            Some(choice) => spawn(&choice.command).map(|()| RESPONSE_SUCCESS),
            None => Ok(RESPONSE_CANCELLED),
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.AppChooser")]
impl AppChooserService {
    async fn choose_application(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        _parent_window: &str,
        choices: Vec<&str>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        tracing::debug!(
            "ChooseApplication: app_id={app_id} choices={choices:?} options={options:?}"
        );

        let option = |key| {
            options
                .get(key)
                .and_then(|value| value.downcast_ref::<&str>().ok())
                .with_context(|| format!("missing option {key}"))
        };

        let result = async {
            let uri = option("uri")?;
            let content_type = option("content_type")?;

            // applications pass URIs, programs expect local paths
            let file = Url::parse(uri)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .map_or(uri.to_string(), |path| path.display().to_string());

            self.choose(&file, &self.targets(content_type, &choices))
                .await
        };

        match result.await {
            // the backend launches the application itself, so there are no results for the frontend
            Ok(response) => Ok((response, HashMap::new())),
            Err(err) => {
                tracing::error!("{err:#}");
                Err(fdo::Error::Failed(format!("{err:#}")))
            }
        }
    }
}
