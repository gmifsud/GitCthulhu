//! config_adapter
//! Implements the ConfigStore port for persisting user-defined settings using confy.

use core_domain::{AppSettings, ConfigStore, DomainError, WorktreeTabRetention};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct AppSettingsDto {
    pub theme: String,
    pub repository_shortcuts: Vec<String>,
    /// Absent in settings written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_repo_path: Option<String>,
    /// Absent in settings written before Appearance existed.
    #[serde(default = "default_ui_font")]
    pub ui_font: String,
    #[serde(default = "default_ui_size")]
    pub ui_size: u8,
    /// Absent until the user saves a Kilo token.
    #[serde(default)]
    pub agent_token: String,
    #[serde(default = "default_agent_model")]
    pub agent_model: String,
    /// Absent in settings written before worktree tabs existed.
    #[serde(default)]
    pub worktree_tab_retention: WorktreeTabRetention,
    /// Absent in settings written before the recent list existed.
    #[serde(default)]
    pub recent_repositories: Vec<String>,
}

fn default_ui_font() -> String {
    AppSettings::DEFAULT_UI_FONT.to_string()
}

fn default_ui_size() -> u8 {
    AppSettings::DEFAULT_UI_SIZE
}

fn default_agent_model() -> String {
    AppSettings::DEFAULT_AGENT_MODEL.to_string()
}

impl Default for AppSettingsDto {
    fn default() -> Self {
        Self {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec![],
            last_repo_path: None,
            ui_font: default_ui_font(),
            ui_size: default_ui_size(),
            agent_token: String::new(),
            agent_model: default_agent_model(),
            worktree_tab_retention: WorktreeTabRetention::default(),
            recent_repositories: Vec::new(),
        }
    }
}

impl From<AppSettingsDto> for AppSettings {
    fn from(dto: AppSettingsDto) -> Self {
        Self {
            theme: dto.theme,
            repository_shortcuts: dto.repository_shortcuts,
            last_repo_path: dto.last_repo_path,
            ui_font: dto.ui_font,
            ui_size: dto.ui_size,
            agent_token: dto.agent_token,
            agent_model: dto.agent_model,
            worktree_tab_retention: dto.worktree_tab_retention,
            recent_repositories: dto.recent_repositories,
        }
    }
}

impl From<&AppSettings> for AppSettingsDto {
    fn from(domain: &AppSettings) -> Self {
        Self {
            theme: domain.theme.clone(),
            repository_shortcuts: domain.repository_shortcuts.clone(),
            last_repo_path: domain.last_repo_path.clone(),
            ui_font: domain.ui_font.clone(),
            ui_size: domain.ui_size,
            agent_token: domain.agent_token.clone(),
            agent_model: domain.agent_model.clone(),
            worktree_tab_retention: domain.worktree_tab_retention,
            recent_repositories: domain.recent_repositories.clone(),
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

        Ok(dto.into())
    }

    fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError> {
        confy::store(&self.app_name, None, AppSettingsDto::from(settings))
            .map_err(|e| DomainError::Unknown(format!("Failed to save config: {}", e)))
    }

    fn watch_settings<F>(&self, callback: F) -> Result<(), DomainError>
    where
        F: Fn(AppSettings) + Send + 'static,
    {
        use notify::{EventKind, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let path = confy::get_configuration_file_path(&self.app_name, None)
            .map_err(|e| DomainError::Unknown(format!("Failed to get config path: {}", e)))?;

        let app_name = self.app_name.clone();

        std::thread::spawn(move || {
            let (tx, rx) = channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();

            watcher.watch(&path, RecursiveMode::NonRecursive).unwrap();

            for event in rx.into_iter().flatten() {
                if matches!(event.kind, EventKind::Modify(_)) {
                    if let Ok(dto) = confy::load::<AppSettingsDto>(&app_name, None) {
                        callback(dto.into());
                    }
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_app_name(label: &str) -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!("git_cthulhu_{label}_{}_{n}", std::process::id())
    }

    fn with_store(label: &str, body: impl FnOnce(&ConfyConfigStore)) {
        let name = unique_app_name(label);
        let path = confy::get_configuration_file_path(&name, None).expect("config path");
        let _ = std::fs::remove_file(&path);
        let store = ConfyConfigStore::new(&name);
        body(&store);
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }

    #[test]
    fn last_repo_path_round_trips_and_keeps_the_other_settings() {
        with_store("round_trip", |store| {
            let mut settings = store.load_settings().expect("default settings");
            assert_eq!(settings.last_repo_path, None);
            settings.theme = "Other".to_string();
            settings.repository_shortcuts = vec!["one".to_string()];
            settings.last_repo_path = Some(r"D:\repo".to_string());

            store.save_settings(&settings).expect("save");

            assert_eq!(store.load_settings().expect("reload"), settings);
        });
    }

    #[test]
    fn a_settings_file_without_last_repo_path_still_loads() {
        with_store("old_file", |store| {
            let path =
                confy::get_configuration_file_path(&store.app_name, None).expect("config path");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("config dir");
            }
            std::fs::write(
                &path,
                "theme = \"DeepFocus\"\nrepository_shortcuts = [\"one\"]\n",
            )
            .expect("write old settings");

            let settings = store.load_settings().expect("load old settings");

            assert_eq!(settings.theme, "DeepFocus");
            assert_eq!(settings.repository_shortcuts, vec!["one".to_string()]);
            assert_eq!(settings.last_repo_path, None);
            assert_eq!(settings.ui_font, AppSettings::DEFAULT_UI_FONT);
            assert_eq!(settings.ui_size, AppSettings::DEFAULT_UI_SIZE);
            assert_eq!(settings.agent_token, "");
            assert_eq!(settings.agent_model, AppSettings::DEFAULT_AGENT_MODEL);
            assert_eq!(settings.worktree_tab_retention, WorktreeTabRetention::Partial);
        });
    }

    #[test]
    fn worktree_tab_retention_defaults_to_partial_and_round_trips() {
        with_store("retention", |store| {
            let mut settings = store.load_settings().expect("default");
            assert_eq!(settings.worktree_tab_retention, WorktreeTabRetention::Partial);
            for mode in [
                WorktreeTabRetention::Full,
                WorktreeTabRetention::PathOnly,
                WorktreeTabRetention::Partial,
            ] {
                settings.worktree_tab_retention = mode;
                store.save_settings(&settings).expect("save");
                assert_eq!(store.load_settings().expect("reload"), settings);
            }
        });
    }

    #[test]
    fn the_agent_token_and_model_round_trip() {
        with_store("agent", |store| {
            let mut settings = store.load_settings().expect("default");
            settings.agent_token = "kilo-token".to_string();
            settings.agent_model = "kilo-auto/free".to_string();
            settings.last_repo_path = Some(r"D:\repo".to_string());
            store.save_settings(&settings).expect("save");
            assert_eq!(store.load_settings().expect("reload"), settings);
        });
    }
}
