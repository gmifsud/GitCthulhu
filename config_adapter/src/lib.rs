//! config_adapter
//! Implements the ConfigStore port for persisting user-defined settings using confy.

use core_domain::{AppSettings, ConfigStore, DomainError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct AppSettingsDto {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
        }
    }
}

impl From<AppSettingsDto> for AppSettings {
    fn From(dto: AppSettingsDto) -> Self {
        Self {
            theme: dto.theme,
            repository_shortcuts: dto.repository_shortcuts,
        }
    }
}

impl From<&AppSettings> for AppSettingsDto {
    fn From(domain: &AppSettings) -> Self {
        Self {
            theme: domain.theme.clone(),
            repository_shortcuts: domain.repository_shortcuts.clone(),
        }
    }
}

pub struct ConfyConfigStore {
    app_name: String,
}

impl ConfyConfigStore {
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
        }
    }
}

impl ConfigStore for ConfyConfigStore {
    fn load_settings(&self) -> Result<AppSettings, DomainError> {
        let dto: AppSettingsDto = confy::load(&self.app_name, None)
            .map_err(|e| DomainError::Unknown(format!("Failed to load config: {}", e)))?;
        
        Ok(AppSettings {
            theme: dto.theme,
            repository_shortcuts: dto.repository_shortcuts,
        })
    }

    fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError> {
        let dto = AppSettingsDto {
            theme: settings.theme.clone(),
            repository_shortcuts: settings.repository_shortcuts.clone(),
        };
        
        confy::store(&self.app_name, None, dto)
            .map_err(|e| DomainError::Unknown(format!("Failed to save config: {}", e)))
    }

    fn watch_settings<F>(&self, callback: F) -> Result<(), DomainError> 
    where
        F: Fn(AppSettings) + Send + 'static
    {
        use notify::{Watcher, RecursiveMode, EventKind};
        use std::sync::mpsc::channel;

        let path = confy::get_configuration_file_path(&self.app_name, None)
            .map_err(|e| DomainError::Unknown(format!("Failed to get config path: {}", e)))?;

        let app_name = self.app_name.clone();

        std::thread::spawn(move || {
            let (tx, rx) = channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();
            
            watcher.watch(&path, RecursiveMode::NonRecursive).unwrap();

            for res in rx {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        if let Ok(dto) = confy::load::<AppSettingsDto>(&app_name, None) {
                            let new_settings = AppSettings {
                                theme: dto.theme,
                                repository_shortcuts: dto.repository_shortcuts,
                            };
                            callback(new_settings);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
