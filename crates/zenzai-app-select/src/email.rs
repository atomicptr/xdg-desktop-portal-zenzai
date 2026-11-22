use url::form_urlencoded::byte_serialize;
use zenzai_core::ipc::Email;

fn encode(value: &str) -> String {
    byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

pub fn thunderbird_command(command: &[String], uri: &str, email: &Email) -> Vec<String> {
    // Thunderbird rejects mailto attachments; its compose parser has its own quoting rules.
    let mut fields = Vec::new();
    for (key, addresses) in [
        ("to", &email.addresses),
        ("cc", &email.cc),
        ("bcc", &email.bcc),
    ] {
        if !addresses.is_empty() {
            // Unquoted values are decoded only after Thunderbird splits compose fields.
            fields.push(format!("{key}={}", encode(&addresses.join(","))));
        }
    }

    if !email.attachments.is_empty() {
        let attachments = email
            .attachments
            .iter()
            .map(|uri| uri.replace(',', "%2C").replace('\'', "%27"))
            .collect::<Vec<_>>()
            .join(",");
        fields.push(format!("attachment='{attachments}'"));
    }
    fields.push(format!("subject={}", encode(&email.subject)));
    let body = email
        .body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    fields.push(format!("body={}", encode(&body)));

    let mut command: Vec<_> = command
        .iter()
        .filter(|arg| arg.as_str() != uri && !matches!(arg.as_str(), "@@u" | "@@"))
        .cloned()
        .collect();
    command.extend(["-compose".to_string(), fields.join(",")]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_arguments_preserve_launcher_and_escape_nested_fields() {
        let email = Email {
            addresses: vec!["o'brien@example.com".into()],
            subject: "a',body=oops & 日本語".into(),
            body: "<b>plain & 'quoted'</b>\nnext".into(),
            attachments: vec!["file:///tmp/a',b".into(), "file:///tmp/c".into()],
            ..Default::default()
        };
        let command = [
            "flatpak",
            "run",
            "org.mozilla.Thunderbird",
            "@@u",
            "mailto:test",
            "@@",
        ]
        .map(str::to_string);
        let command = thunderbird_command(&command, "mailto:test", &email);
        assert_eq!(
            &command[..4],
            ["flatpak", "run", "org.mozilla.Thunderbird", "-compose"]
        );
        assert_eq!(command.len(), 5);
        let compose = &command[4];
        assert!(compose.contains("to=o%27brien%40example.com"));
        assert!(compose.contains("attachment='file:///tmp/a%27%2Cb,file:///tmp/c'"));
        assert!(compose.contains("subject=a%27%2Cbody%3Doops%20%26%20"));
        assert!(compose.contains("body=%26lt%3Bb%26gt%3Bplain"));
    }

    #[test]
    fn recipients_cannot_inject_compose_fields() {
        let email = Email {
            addresses: vec![
                "victim'".into(),
                "attachment=file:///etc/passwd".into(),
                "x='".into(),
            ],
            ..Default::default()
        };
        let command = thunderbird_command(&["thunderbird".into()], "mailto:", &email);
        // Thunderbird splits unquoted fields on commas, then URI-decodes their values.
        let fields = command.last().unwrap().replace(',', "&");
        let fields: Vec<_> = url::form_urlencoded::parse(fields.as_bytes()).collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].0, "to");
        assert_eq!(fields[0].1, email.addresses.join(","));
        assert!(!fields.iter().any(|(key, _)| key == "attachment"));
    }
}
