//! git_cthulhu binary entry point.
//! Constructs adapters and injects them into the pure `gui_app` library.
//! This is the ONLY place in the application that imports `git_adapter` directly.

use git_adapter::GitoxideAdapter;

fn main() -> iced::Result {
    // TODO: [DEBT] Replace argv path with config_adapter::load_settings().last_repo_path
    match std::env::args().nth(1) {
        Some(path) => gui_app::run(Box::new(GitoxideAdapter::new(&path))),
        None => gui_app::run_welcome(),
    }
}
