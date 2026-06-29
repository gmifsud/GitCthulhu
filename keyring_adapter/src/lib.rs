//! keyring_adapter
//! Implements the `CredentialStore` port using native OS secure enclaves.

use core_domain::{CredentialStore, DomainError};
use keyring::Entry;

pub struct NativeKeyring {
    service_name: String,
}

impl NativeKeyring {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
        }
    }
    
    fn get_entry(&self, user: &str) -> Result<Entry, DomainError> {
        Entry::new(&self.service_name, user)
            .map_err(|e| DomainError::Unknown(format!("Keyring error: {}", e)))
    }
}

impl CredentialStore for NativeKeyring {
    fn save_token(&self, user: &str, token: &str) -> Result<(), DomainError> {
        let entry = self.get_entry(user)?;
        entry.set_password(token)
            .map_err(|e| DomainError::Unknown(format!("Failed to save token: {}", e)))
    }

    fn get_token(&self, user: &str) -> Result<String, DomainError> {
        let entry = self.get_entry(user)?;
        entry.get_password()
            .map_err(|_| DomainError::AuthenticationFailed)
    }

    fn delete_token(&self, user: &str) -> Result<(), DomainError> {
        let entry = self.get_entry(user)?;
        entry.delete_password()
            .map_err(|e| DomainError::Unknown(format!("Failed to delete token: {}", e)))
    }
}
