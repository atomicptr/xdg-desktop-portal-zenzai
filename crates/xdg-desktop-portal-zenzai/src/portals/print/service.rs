use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, MutexGuard, PoisonError},
};

use anyhow::Result;
use tokio::sync::watch;
use zbus::{Fd, ObjectPath, OwnedVariant, Variant, fdo, interface};
use zenzai_core::ipc::{PrintRequest, PrintResponse};

use super::{
    dialog::{self, Outcome, PreparedPrint},
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

    // Recover poisoned locks; no token operation holds the lock across an await.
    fn tokens(&self) -> MutexGuard<'_, Tokens> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn run_dialog(&self, request: &PrintRequest) -> Result<PreparedPrint, u32> {
        let command = self.command.borrow().clone();
        Self::dialog_result(dialog::run(&command, request).await)
    }

    fn dialog_result(result: Result<Outcome>) -> Result<PreparedPrint, u32> {
        match result {
            Ok(Outcome::Accepted(prepared)) => Ok(prepared),
            Ok(Outcome::Cancelled) => Err(RESPONSE_CANCELLED),
            Err(err) => {
                tracing::error!("print dialog: {err:#}");
                Err(RESPONSE_OTHER)
            }
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.Print")]
impl PrintService {
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all, fields(request = %handle, app_id = %app_id))]
    async fn prepare_print(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        settings: HashMap<&str, Variant<'_>>,
        page_setup: HashMap<&str, Variant<'_>>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let _ = parent_window;
        tracing::debug!(
            settings_keys = ?settings.keys().collect::<Vec<_>>(),
            page_setup_keys = ?page_setup.keys().collect::<Vec<_>>(),
            option_keys = ?options.keys().collect::<Vec<_>>(),
            "received prepare print request"
        );

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

        let prepared = match self.run_dialog(&request).await {
            Ok(prepared) => prepared,
            Err(code) => {
                tracing::debug!(response = code, "prepare print request completed");
                return Ok((code, HashMap::new()));
            }
        };

        let settings = settings_to_variant(&prepared.response.settings);
        let page_setup = setup_to_variant(&prepared.response.page_setup);
        let token = self.tokens().insert(app_id, prepared);

        let results = HashMap::from([
            ("settings".to_string(), settings),
            ("page-setup".to_string(), page_setup),
            ("token".to_string(), token.into()),
        ]);

        tracing::debug!(response = RESPONSE_SUCCESS, "prepared print settings");
        Ok((RESPONSE_SUCCESS, results))
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all, fields(request = %handle, app_id = %app_id))]
    async fn print(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        fd: Fd<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, OwnedVariant>)> {
        let _ = parent_window;

        let token = match options
            .get("token")
            .map(Variant::downcast_ref::<u32>)
            .transpose()
        {
            Ok(token) => token.filter(|token| *token != 0),
            Err(_) => return Ok((RESPONSE_OTHER, HashMap::new())),
        };

        let prepared = match token {
            Some(token) => match self.tokens().take(app_id, token) {
                Some(prepared) => Some(prepared),
                None => return Ok((RESPONSE_CANCELLED, HashMap::new())),
            },
            None => None,
        };

        tracing::debug!(
            option_keys = ?options.keys().collect::<Vec<_>>(),
            token_supplied = token.is_some(),
            prepared = prepared.is_some(),
            "received print request"
        );

        let document = match dialog::copy_document(fd).await {
            Ok(document) => document,
            Err(err) => {
                tracing::error!("print: could not read document: {err:#}");
                return Ok((RESPONSE_OTHER, HashMap::new()));
            }
        };

        let prerendered = prepared.is_some();
        let PreparedPrint { response, session } = prepared.unwrap_or(PreparedPrint {
            response: PrintResponse::default(),
            session: None,
        });
        let PrintResponse {
            settings,
            page_setup,
            ..
        } = response;

        let request = PrintRequest {
            title: title.to_string(),
            document: Some(document.path().to_path_buf()),
            interactive: !prerendered || session.is_some(),
            prerendered,
            supported_output_formats: supported_output_formats(&options),
            settings,
            page_setup,
            ..Default::default()
        };

        let result = match session {
            Some(session) => Self::dialog_result(session.resume(request, document).await),
            None => self.run_dialog(&request).await,
        };
        let code = result.map_or_else(|code| code, |_| RESPONSE_SUCCESS);
        tracing::debug!(response = code, "print request completed");
        Ok((code, HashMap::new()))
    }
}

fn supported_output_formats(options: &HashMap<&str, Variant<'_>>) -> Option<Vec<String>> {
    options
        .get("supported_output_file_formats")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| value.downcast::<Vec<String>>().ok())
}
