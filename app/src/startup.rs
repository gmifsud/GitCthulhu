//! Choose which repository the process opens, and remember that choice.
//! The window never sees the config store.

use core_domain::{AppSettings, ConfigStore};

/// Argument wins. Otherwise the remembered path, when `is_repository` accepts it.
pub fn launch_path(
    argument: Option<&str>,
    settings: &AppSettings,
    is_repository: impl Fn(&str) -> bool,
) -> Option<String> {
    if let Some(path) = argument {
        return Some(path.to_string());
    }
    let path = settings.last_repo_path.as_deref()?;
    if path.trim().is_empty() || !is_repository(path) {
        None
    } else {
        Some(path.to_string())
    }
}

/// Store `path` as the last repository. A failed load or save leaves the file unchanged
/// apart from what the store itself already wrote, and never blocks opening.
pub fn remember(store: &impl ConfigStore, path: &str) {
    let Ok(mut settings) = store.load_settings() else {
        return;
    };
    if settings.last_repo_path.as_deref() == Some(path) {
        return;
    }
    settings.last_repo_path = Some(path.to_string());
    let _ = store.save_settings(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::DomainError;
    use std::sync::Mutex;

    struct MemoryStore {
        loaded: Mutex<Result<AppSettings, DomainError>>,
        saved: Mutex<Option<AppSettings>>,
        saves: Mutex<u32>,
    }

    impl MemoryStore {
        fn holding(settings: AppSettings) -> Self {
            Self {
                loaded: Mutex::new(Ok(settings)),
                saved: Mutex::new(None),
                saves: Mutex::new(0),
            }
        }

        fn broken() -> Self {
            Self {
                loaded: Mutex::new(Err(DomainError::Unknown("unreadable".into()))),
                saved: Mutex::new(None),
                saves: Mutex::new(0),
            }
        }
    }

    impl ConfigStore for MemoryStore {
        fn load_settings(&self) -> Result<AppSettings, DomainError> {
            self.loaded.lock().expect("settings lock").clone()
        }

        fn save_settings(&self, settings: &AppSettings) -> Result<(), DomainError> {
            *self.saves.lock().expect("save count") += 1;
            *self.saved.lock().expect("saved settings") = Some(settings.clone());
            Ok(())
        }

        fn watch_settings<F>(&self, _callback: F) -> Result<(), DomainError>
        where
            F: Fn(AppSettings) + Send + 'static,
        {
            Ok(())
        }
    }

    fn settings_with(path: Option<&str>) -> AppSettings {
        AppSettings {
            theme: "DeepFocus".to_string(),
            repository_shortcuts: vec!["one".to_string()],
            last_repo_path: path.map(str::to_string),
        }
    }

    #[test]
    fn an_argument_wins_over_the_remembered_path() {
        let settings = settings_with(Some(r"D:\saved"));
        let path = launch_path(Some(r"D:\argv"), &settings, |_| panic!("should not probe"));
        assert_eq!(path.as_deref(), Some(r"D:\argv"));
    }

    #[test]
    fn no_argument_opens_the_remembered_repository() {
        let settings = settings_with(Some(r"D:\saved"));
        let path = launch_path(None, &settings, |path| path == r"D:\saved");
        assert_eq!(path.as_deref(), Some(r"D:\saved"));
    }

    #[test]
    fn a_remembered_path_that_is_not_a_repository_opens_the_welcome_screen() {
        let settings = settings_with(Some(r"D:\gone"));
        assert_eq!(launch_path(None, &settings, |_| false), None);
    }

    #[test]
    fn a_blank_remembered_path_opens_the_welcome_screen() {
        let settings = settings_with(Some("   "));
        assert_eq!(
            launch_path(None, &settings, |_| panic!("should not probe")),
            None
        );
        assert_eq!(
            launch_path(None, &AppSettings::default(), |_| panic!(
                "should not probe"
            )),
            None
        );
    }

    #[test]
    fn remember_keeps_the_other_settings() {
        let store = MemoryStore::holding(settings_with(None));
        remember(&store, r"D:\repo");
        let saved = store
            .saved
            .lock()
            .expect("saved settings")
            .clone()
            .expect("one save");
        assert_eq!(saved.theme, "DeepFocus");
        assert_eq!(saved.repository_shortcuts, vec!["one".to_string()]);
        assert_eq!(saved.last_repo_path.as_deref(), Some(r"D:\repo"));
    }

    #[test]
    fn remember_skips_a_path_that_is_already_stored() {
        let store = MemoryStore::holding(settings_with(Some(r"D:\repo")));
        remember(&store, r"D:\repo");
        assert_eq!(*store.saves.lock().expect("save count"), 0);
    }

    #[test]
    fn remember_does_nothing_when_settings_cannot_be_read() {
        let store = MemoryStore::broken();
        remember(&store, r"D:\repo");
        assert_eq!(*store.saves.lock().expect("save count"), 0);
    }
}
