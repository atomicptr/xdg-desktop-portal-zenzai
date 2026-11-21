use std::collections::HashMap;

use zbus::{OwnedObjectPath, Variant, fdo, interface};

pub struct SecretService;

#[interface(name = "org.freedesktop.impl.portal.Secret")]
impl SecretService {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        1
    }

    async fn retrieve_secret(
        &self,
        handle: &str,
        options: HashMap<&str, Variant<'_>>,
    ) -> fdo::Result<OwnedObjectPath> {
        tracing::info!("retrieve secret: {} {:?}", handle, options);
        tracing::error!("not yet implemented");
        Err(fdo::Error::Failed("not yet implemented".to_string()))
    }
}
