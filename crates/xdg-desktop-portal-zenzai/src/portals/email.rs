use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;
use url::Url;
use zbus::{ObjectPath, OwnedVariant, Variant, interface};
use zenzai_core::ipc::{AppSelectRequest, Email};

use super::appchooser::service::AppChooserService;

const MAILTO_TYPE: &str = "x-scheme-handler/mailto";

#[derive(Debug, Default, Deserialize)]
pub struct EmailConfig {
    pub enabled: bool,
    pub command: Option<String>,
}

pub struct EmailService {
    pub chooser: AppChooserService,
}

#[interface(name = "org.freedesktop.impl.portal.Email")]
impl EmailService {
    #[tracing::instrument(level = "debug", skip_all, fields(method = "ComposeEmail", %handle, %app_id))]
    async fn compose_email(
        &self,
        handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        options: HashMap<&str, Variant<'_>>,
    ) -> (u32, HashMap<String, OwnedVariant>) {
        tracing::debug!(option_keys = ?options.keys().collect::<Vec<_>>(), "received email request");
        let _ = parent_window;
        let result = async {
            let request = request(&options)?;
            self.chooser.choose(&request).await
        }
        .await;

        let response = if let Ok(response) = result {
            response
        } else {
            // Errors from launchers can contain the complete message URI.
            tracing::error!("could not compose email");
            2
        };
        tracing::debug!(response, "email request completed");
        (response, HashMap::new())
    }
}

fn request(options: &HashMap<&str, Variant<'_>>) -> Result<AppSelectRequest> {
    let string = |key| -> Result<String> {
        options.get(key).map_or_else(
            || Ok(String::new()),
            |value| Ok(value.downcast_ref::<&str>()?.to_string()),
        )
    };
    let strings = |key| -> Result<Vec<String>> {
        options.get(key).map_or_else(
            || Ok(Vec::new()),
            |value| Ok(Vec::<String>::try_from(value.try_clone()?)?),
        )
    };
    let mut addresses = strings("addresses")?;
    let address = string("address")?;
    if !address.is_empty() {
        addresses.insert(0, address);
    }

    let email = Email {
        addresses,
        cc: strings("cc")?,
        bcc: strings("bcc")?,
        subject: string("subject")?,
        body: string("body")?,
        attachments: strings("attachments")?,
    };
    let mut uri = Url::parse("mailto:")?;
    {
        let mut query = uri.query_pairs_mut();
        for (key, values) in [
            ("to", &email.addresses),
            ("cc", &email.cc),
            ("bcc", &email.bcc),
            ("attachment", &email.attachments),
        ] {
            for value in values {
                query.append_pair(key, value);
            }
        }
        query.append_pair("subject", &email.subject);
        query.append_pair("body", &email.body);
    }

    Ok(AppSelectRequest {
        // mailto uses URI escaping, not form encoding's '+' for spaces.
        uri: uri.to_string().replace('+', "%20"),
        content_type: MAILTO_TYPE.to_string(),
        activation_token: options
            .get("activation_token")
            .map(|value| value.downcast_ref::<&str>().map(str::to_owned))
            .transpose()?,
        email: Some(email),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_recipients_and_encodes_message_and_attachments() {
        let options = HashMap::from([
            ("address", Variant::from("first+tag@example.com")),
            ("addresses", Variant::from(vec!["second@example.com"])),
            ("cc", Variant::from(vec!["copy@example.com"])),
            ("bcc", Variant::from(vec!["hidden@example.com"])),
            ("subject", Variant::from("Hello & 日本語?")),
            ("body", Variant::from("one\r\ntwo+three #four")),
            (
                "attachments",
                Variant::from(vec!["file:///tmp/a%20b", "file:///tmp/c%26d"]),
            ),
        ]);
        let request = request(&options).unwrap();
        let uri = Url::parse(&request.uri).unwrap();
        let pairs: Vec<_> = uri.query_pairs().collect();
        assert_eq!(pairs.iter().filter(|(key, _)| key == "to").count(), 2);
        assert_eq!(
            pairs.iter().filter(|(key, _)| key == "attachment").count(),
            2
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "subject" && value == "Hello & 日本語?")
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "body" && value == "one\r\ntwo+three #four")
        );
        assert!(request.uri.contains("%20"));
        assert!(!request.uri.contains('+'));
        assert!(request.email.unwrap().addresses[0].starts_with("first+tag"));
        assert!(super::request(&HashMap::new()).is_ok());
        assert!(super::request(&HashMap::from([("subject", Variant::from(42u32))])).is_err());
    }
}
