use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, MutexGuard, PoisonError},
};

use tokio::sync::watch;
use zbus::{Fd, ObjectPath, OwnedVariant, Variant, fdo, interface};
use zenzai_core::ipc::{PrintRequest, PrintResponse};

use super::{
    dialog::{self, Outcome},
    tokens::Tokens,
    variant::{
        get_bool, settings_from_dict, settings_to_variant, setup_from_dict, setup_to_variant,
    },
};

const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;
const RESPONSE_OTHER: u32 = 2;

pub struct PrintService {
    command: watch::Receiver<PathBuf>,
    tokens: Mutex<Tokens>,
}

impl PrintService {
    pub fn new(command: watch::Receiver<PathBuf>) -> Self {
        Self {
            command,
            tokens: Mutex::default(),
        }
    }

    // the lock is never held across an await, a poisoned lock only means a panicked call
    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn run_dialog(&self, request: &PrintRequest) -> Result<Outcome, u32> {
        let command = self.command.borrow().clone();
        dialog::run(&command, request).await.map_err(|err| {
            tracing::error!("print dialog: {err:#}");
            RESPONSE_OTHER
        })
    }
}

#[interface(name = "org.freedesktop.impl.portal.Print")]
impl PrintService {
    #[allow(clippy::too_many_arguments)]
    async fn prepare_print(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        _parent_window: &str,
        title: &str,
        settings: HashMap<&str, Variant<'_>>,
        page_setup: HashMap<&str, Variant<'_>>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        tracing::debug!("PreparePrint: app_id={app_id} title={title} options={options:?}");

        let request = PrintRequest {
            title: title.to_string(),
            settings: settings_from_dict(&settings),
            page_setup: setup_from_dict(&page_setup),
            interactive: true,
            accept_label: options
                .get("accept_label")
                .and_then(|value| value.downcast_ref::<&str>().ok())
                .map(|label| label.replace('_', "")),
            has_current_page: get_bool(&options, "has_current_page"),
            has_selected_pages: get_bool(&options, "has_selected_pages"),
            supported_output_formats: supported_output_formats(&options),
            ..Default::default()
        };

        let response = match self.run_dialog(&request).await {
            Ok(Outcome::Accepted(response)) => response,
            Ok(Outcome::Cancelled) => return Ok((RESPONSE_CANCELLED, HashMap::new())),
            Err(code) => return Ok((code, HashMap::new())),
        };

        let settings = settings_to_variant(&response.settings);
        let page_setup = setup_to_variant(&response.page_setup);
        let token = self.tokens().insert(app_id, response);

        let results = HashMap::from([
            ("settings".to_string(), settings),
            ("page-setup".to_string(), page_setup),
            ("token".to_string(), token.into()),
        ]);

        Ok((RESPONSE_SUCCESS, results))
    }

    #[allow(clippy::too_many_arguments)]
    async fn print(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        _parent_window: &str,
        title: &str,
        fd: Fd<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        tracing::debug!("Print: app_id={app_id} title={title} options={options:?}");

        let token = options
            .get("token")
            .and_then(|value| value.downcast_ref::<u32>().ok());

        let prepared = token.and_then(|token| self.tokens().take(app_id, token));

        let document = match dialog::copy_document(fd).await {
            Ok(document) => document,
            Err(err) => {
                tracing::error!("print: could not read document: {err:#}");
                return Ok((RESPONSE_OTHER, HashMap::new()));
            }
        };

        let prerendered = prepared.is_some();
        let PrintResponse {
            settings,
            page_setup,
        } = prepared.unwrap_or_default();

        let request = PrintRequest {
            title: title.to_string(),
            document: Some(document.path().to_path_buf()),
            interactive: !prerendered,
            prerendered,
            supported_output_formats: supported_output_formats(&options),
            settings,
            page_setup,
            ..Default::default()
        };

        match self.run_dialog(&request).await {
            Ok(Outcome::Accepted(_)) => Ok((RESPONSE_SUCCESS, HashMap::new())),
            Ok(Outcome::Cancelled) => Ok((RESPONSE_CANCELLED, HashMap::new())),
            Err(code) => Ok((code, HashMap::new())),
        }
    }
}

fn supported_output_formats(options: &HashMap<&str, Variant<'_>>) -> Option<Vec<String>> {
    options
        .get("supported_output_file_formats")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| value.downcast::<Vec<String>>().ok())
}
