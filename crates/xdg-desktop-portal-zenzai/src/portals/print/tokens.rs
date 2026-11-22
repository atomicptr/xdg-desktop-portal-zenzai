use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use zenzai_core::ipc::PrintResponse;

const TOKEN_LIFETIME: Duration = Duration::from_secs(300);

struct Entry {
    app_id: String,
    response: PrintResponse,
    created: Instant,
}

#[derive(Default)]
pub struct Tokens {
    entries: HashMap<u32, Entry>,
}

impl Tokens {
    pub fn insert(&mut self, app_id: &str, response: PrintResponse) -> u32 {
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
                response,
                created: Instant::now(),
            },
        );

        token
    }

    // single use and bound to the requesting app, so no other app can print with these settings
    pub fn take(&mut self, app_id: &str, token: u32) -> Option<PrintResponse> {
        let entry = self.entries.remove(&token)?;

        (entry.app_id == app_id && entry.created.elapsed() < TOKEN_LIFETIME)
            .then_some(entry.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_single_use_and_bound_to_the_app() {
        let mut tokens = Tokens::default();

        let token = tokens.insert("org.example.App", PrintResponse::default());
        assert!(tokens.take("org.example.Other", token).is_none());

        let token = tokens.insert("org.example.App", PrintResponse::default());
        assert!(tokens.take("org.example.App", token).is_some());
        assert!(tokens.take("org.example.App", token).is_none());
    }
}
