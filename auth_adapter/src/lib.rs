//! auth_adapter
//! Implements the OAuthServer port for receiving provider authentication tokens.

use core_domain::{DomainError, OAuthServer};
use std::sync::{Arc, Mutex};

pub struct LocalOAuthServer {
    token_store: Arc<Mutex<Option<String>>>,
}

impl LocalOAuthServer {
    pub fn new() -> Self {
        Self {
            token_store: Arc::new(Mutex::new(None)),
        }
    }
}

impl OAuthServer for LocalOAuthServer {
    fn start_loopback(&self, _port: u16) -> Result<(), DomainError> {
        Ok(())
    }

    fn stop_loopback(&self) -> Result<(), DomainError> {
        Ok(())
    }

    fn get_callback_token(&self) -> Result<Option<String>, DomainError> {
        let store = self.token_store.lock().unwrap();
        Ok(store.clone())
    }
}
