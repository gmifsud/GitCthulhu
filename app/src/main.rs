//! git_cthulhu binary entry point.
//! Constructs adapters and injects them into the pure `gui_app` library.
//! This is the ONLY place in the application that imports `git_adapter` directly.

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

fn opener(store: Arc<ConfyConfigStore>) -> gui_app::RepositoryOpener {
    Arc::new(move |path| {
        remember(store.as_ref(), path);
        Ok(Arc::new(GitoxideAdapter::new(path)))
    })
}

fn main() -> iced::Result {
    let store = Arc::new(ConfyConfigStore::new(APP_NAME));
    let open = opener(Arc::clone(&store));
    let argument = std::env::args().nth(1);
    if let Some(path) = argument.as_deref() {
        remember(store.as_ref(), path);
    }
    let settings = store.load_settings().unwrap_or_default();
    match launch_path(argument.as_deref(), &settings, repository_exists) {
        Some(path) => gui_app::run(Box::new(GitoxideAdapter::new(&path)), open),
        None => gui_app::run_welcome(open),
    }
}
