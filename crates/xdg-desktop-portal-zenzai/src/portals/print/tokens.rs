use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use super::dialog::PreparedPrint;

pub(super) const TOKEN_LIFETIME: Duration = Duration::from_secs(300);

struct Entry {
    app_id: String,
    prepared: PreparedPrint,
    created: Instant,
}

#[derive(Default)]
pub struct Tokens {
    entries: HashMap<u32, Entry>,
}

impl Tokens {
    pub fn insert(&mut self, app_id: &str, prepared: PreparedPrint) -> u32 {
        self.entries
            .retain(|_, entry| entry.created.elapsed() < TOKEN_LIFETIME);

        let token = loop {
            let token = rand::random::<u32>();

            if token != 0 && !self.entries.contains_key(&token) {
                break token;
            }
        };

        self.entries.insert(
            token,
            Entry {
                app_id: app_id.to_string(),
                prepared,
                created: Instant::now(),
            },
        );

        token
    }

    // single use and bound to the requesting app, so no other app can print with these settings
    pub fn take(&mut self, app_id: &str, token: u32) -> Option<PreparedPrint> {
        let entry = self.entries.remove(&token)?;

        (entry.app_id == app_id && entry.created.elapsed() < TOKEN_LIFETIME)
            .then_some(entry.prepared)
    }
}

#[cfg(test)]
mod tests {
    use zenzai_core::ipc::{PrintAction, PrintResponse};

    use super::*;

    fn prepared() -> PreparedPrint {
        PreparedPrint {
            response: PrintResponse::default(),
            session: None,
        }
    }

    #[test]
    fn tokens_are_single_use_and_bound_to_the_app() {
        let mut tokens = Tokens::default();

        let token = tokens.insert("org.example.App", prepared());
        assert!(tokens.take("org.example.Other", token).is_none());

        let token = tokens.insert("org.example.App", prepared());
        let prepared = tokens.take("org.example.App", token).unwrap();
        assert_eq!(prepared.response.action, PrintAction::Submit);
        assert!(prepared.session.is_none());
        assert!(tokens.take("org.example.App", token).is_none());
    }

    #[test]
    fn expired_tokens_cannot_be_used() {
        let mut tokens = Tokens::default();
        let token = tokens.insert("org.example.App", prepared());
        tokens.entries.get_mut(&token).unwrap().created =
            Instant::now().checked_sub(TOKEN_LIFETIME).unwrap();
        assert!(tokens.take("org.example.App", token).is_none());

        let token = tokens.insert("org.example.App", prepared());
        tokens.entries.get_mut(&token).unwrap().created =
            Instant::now().checked_sub(TOKEN_LIFETIME).unwrap();
        tokens.insert("org.example.App", prepared());
        assert!(!tokens.entries.contains_key(&token));
    }
}
