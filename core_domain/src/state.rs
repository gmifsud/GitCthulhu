use super::{build_dag, AppSettings, Branch, Commit, DagNode, DomainError, RepoStatus, Stash, Tag};

/// Events applied to [`AppState`].
#[derive(Debug, Clone)]
pub enum ViewEvent {
    StatusUpdated(RepoStatus),
    LogUpdated(Vec<Commit>),
    BranchesUpdated(Vec<Branch>),
    StashesUpdated(Vec<Stash>),
    TagsUpdated(Vec<Tag>),
    SettingsUpdated(AppSettings),
    ErrorOccurred(DomainError),
}

/// Repository view state. `apply_event` is the only writer.
pub struct AppState {
    pub current_status: Option<RepoStatus>,
    pub commits: Vec<Commit>,
    pub dag_nodes: Vec<DagNode>,
    pub branches: Vec<Branch>,
    pub stashes: Vec<Stash>,
    pub tags: Vec<Tag>,
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
            stashes: vec![],
            tags: vec![],
            loading: false,
            settings: AppSettings::default(),
        }
    }

    pub fn apply_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::StatusUpdated(status) => {
                self.current_status = Some(status);
                sync_head_tracking_from_status(self);
                self.loading = false;
            }
            ViewEvent::LogUpdated(commits) => {
                self.dag_nodes = build_dag(&commits);
                self.commits = commits;
                self.loading = false;
            }
            ViewEvent::BranchesUpdated(branches) => {
                self.branches = branches;
                sync_head_tracking_from_status(self);
                self.loading = false;
            }
            ViewEvent::StashesUpdated(stashes) => {
                self.stashes = stashes;
                self.loading = false;
            }
            ViewEvent::TagsUpdated(tags) => {
                self.tags = tags;
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

/// Prefer `git status` ahead/behind on HEAD when the branch list left it at zero.
fn sync_head_tracking_from_status(state: &mut AppState) {
    let Some(tracking) = state
        .current_status
        .as_ref()
        .and_then(|status| status.tracking.as_ref())
    else {
        return;
    };
    let ahead = tracking.ahead;
    let behind = tracking.behind;
    if let Some(branch) = state
        .branches
        .iter_mut()
        .find(|branch| branch.is_head && !branch.is_remote)
    {
        if branch.ahead == 0 && branch.behind == 0 {
            branch.ahead = ahead;
            branch.behind = behind;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
