use super::{build_dag, AppSettings, Branch, Commit, DagNode, DomainError, RepoStatus};

/// Events applied to [`AppState`].
#[derive(Debug, Clone)]
pub enum ViewEvent {
    StatusUpdated(RepoStatus),
    LogUpdated(Vec<Commit>),
    BranchesUpdated(Vec<Branch>),
    SettingsUpdated(AppSettings),
    ErrorOccurred(DomainError),
}

/// Repository view state. `apply_event` is the only writer.
pub struct AppState {
    pub current_status: Option<RepoStatus>,
    pub commits: Vec<Commit>,
    pub dag_nodes: Vec<DagNode>,
    pub branches: Vec<Branch>,
    pub loading: bool,
    pub settings: AppSettings,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_status: None,
            commits: vec![],
            dag_nodes: vec![],
            branches: vec![],
            loading: false,
            settings: AppSettings::default(),
        }
    }

    pub fn apply_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::StatusUpdated(status) => {
                self.current_status = Some(status);
                self.loading = false;
            }
            ViewEvent::LogUpdated(commits) => {
                self.dag_nodes = build_dag(&commits);
                self.commits = commits;
                self.loading = false;
            }
            ViewEvent::BranchesUpdated(branches) => {
                self.branches = branches;
                self.loading = false;
            }
            ViewEvent::SettingsUpdated(settings) => {
                self.settings = settings;
            }
            ViewEvent::ErrorOccurred(_) => {
                self.loading = false;
            }
        }
    }
}
