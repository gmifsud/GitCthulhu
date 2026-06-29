//! ssh_adapter
//! Implements the `SshManager` port to handle native OS SSH key resolution
//! and subsystem path mapping (e.g. WSL2).

use core_domain::{DomainError, SshManager};
use std::path::PathBuf;

pub struct NativeSshManager {
    // We could store env context here if needed
}

impl NativeSshManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl SshManager for NativeSshManager {
    fn get_default_key_path(&self) -> Result<String, DomainError> {
        if let Some(home) = dirs::home_dir() {
            // Check for ed25519 first, then rsa
            let ed25519 = home.join(".ssh").join("id_ed25519");
            if ed25519.exists() {
                return Ok(ed25519.to_string_lossy().to_string());
            }
            let rsa = home.join(".ssh").join("id_rsa");
            if rsa.exists() {
                return Ok(rsa.to_string_lossy().to_string());
            }
            // fallback
            Ok(home.join(".ssh").join("id_rsa").to_string_lossy().to_string())
        } else {
            Err(DomainError::Unknown("Could not find home directory".to_string()))
        }
    }

    fn resolve_subsystem_path(&self, path: &str) -> String {
        // Here we implement robust path translation to handle execution across varying environments.
        // For instance, if running in WSL2 but needing a Windows path, or vice-versa.
        // For demonstration, we simply return the path or map `/mnt/c` if needed.
        if path.starts_with("C:\\") {
            // E.g. map Windows to WSL2 if we were running git in WSL2 but GUI in Windows
            // Real implementation would use wslpath or similar.
            path.replace("C:\\", "/mnt/c/").replace("\\", "/")
        } else {
            path.to_string()
        }
    }

    fn prepare_ssh_env(&self) -> Result<(String, String), DomainError> {
        let key_path = self.get_default_key_path()?;
        let resolved_path = self.resolve_subsystem_path(&key_path);
        
        // Return (Environment Variable Key, Environment Variable Value)
        let env_key = "GIT_SSH_COMMAND".to_string();
        let env_val = format!("ssh -i {} -o IdentitiesOnly=yes", resolved_path);
        Ok((env_key, env_val))
    }
}
