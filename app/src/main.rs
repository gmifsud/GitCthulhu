//! git_cthulhu binary entry point.
//! Constructs adapters and injects them into the pure `gui_app` library.
//! This is the ONLY place in the application that imports `git_adapter` directly.

mod kilo;
mod startup;

use std::sync::Arc;

use config_adapter::ConfyConfigStore;
use core_domain::ConfigStore;
use git_adapter::GitoxideAdapter;
use startup::{launch_path, remember};

const APP_NAME: &str = "git_cthulhu";

fn repository_exists(path: &str) -> bool {
    GitoxideAdapter::discover(path).is_ok()
}

fn initialiser() -> gui_app::RepositoryInitialiser {
    Arc::new(GitoxideAdapter::init)
}

fn cloner() -> gui_app::RepositoryCloner {
    Arc::new(GitoxideAdapter::clone_repository)
}

fn opener(store: Arc<ConfyConfigStore>) -> gui_app::RepositoryOpener {
    Arc::new(move |path| {
        remember(store.as_ref(), path);
        Ok(Arc::new(GitoxideAdapter::new(path)))
    })
}

/// Write only the font and size, so a remembered path saved by [`remember`] stays put.
fn save_appearance(store: Arc<ConfyConfigStore>) -> gui_app::SettingsSaver {
    Arc::new(move |incoming| {
        let Ok(mut stored) = store.load_settings() else {
            return;
        };
        stored.ui_font = incoming.ui_font;
        stored.ui_size = incoming.ui_size;
        stored.agent_token = incoming.agent_token;
        stored.agent_model = incoming.agent_model;
        let _ = store.save_settings(&stored);
    })
}

fn main() -> iced::Result {
    let store = Arc::new(ConfyConfigStore::new(APP_NAME));
    let open = opener(Arc::clone(&store));
    let initialise = initialiser();
    let clone_repository = cloner();
    let save_settings = save_appearance(Arc::clone(&store));
    let list_models = kilo::lister();
    let suggest_commit = kilo::suggester();
    let argument = std::env::args().nth(1);
    if let Some(path) = argument.as_deref() {
        remember(store.as_ref(), path);
    }
    let settings = store.load_settings().unwrap_or_default();
    match launch_path(argument.as_deref(), &settings, repository_exists) {
        Some(path) => gui_app::run(
            Box::new(GitoxideAdapter::new(&path)),
            path,
            open,
            initialise,
            clone_repository,
            settings,
            save_settings,
            list_models,
            suggest_commit,
        ),
        None => gui_app::run_welcome(
            open,
            initialise,
            clone_repository,
            settings,
            save_settings,
            list_models,
            suggest_commit,
        ),
    }
}
