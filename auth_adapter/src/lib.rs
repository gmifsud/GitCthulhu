//! auth_adapter
//! Implements the OAuthServer port for receiving provider authentication tokens.

use core_domain::{DomainError, OAuthServer};
use std::sync::{Arc, Mutex};
use std::thread;

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
    fn start_loopback(&self, port: u16) -> Result<(), DomainError> {
        // In a real implementation, this would start an HTTP server
        // e.g., using tiny_http on 127.0.0.1:{port} to receive the OAuth callback.
        
        // let server = tiny_http::Server::http(format!("127.0.0.1:{}", port)).unwrap();
        // let token_store = Arc::clone(&self.token_store);
        // thread::spawn(move || {
        //     for request in server.incoming_requests() {
        //         // parse token from request...
        //         *token_store.lock().unwrap() = Some("mock_token".to_string());
        //         let response = tiny_http::Response::from_string("Authentication successful. You can close this window.");
        //         request.respond(response).unwrap();
        //         break; // Stop after receiving
        //     }
        // });
        
        Ok(())
    }

    fn stop_loopback(&self) -> Result<(), DomainError> {
        // Stop the background HTTP server
        Ok(())
    }

    fn get_callback_token(&self) -> Result<Option<String>, DomainError> {
        let store = self.token_store.lock().unwrap();
        Ok(store.clone())
    }
}
